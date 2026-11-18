//! Windows: Core Audio for the volume, mute and outputs; a power-setting
//! notification for brightness changes and WMI to set it.
//!
//! One thread owns the COM objects and runs the commands (coalescing a slider
//! drag to its last value). Windows calls back on its own threads: the volume
//! callback updates the state directly; device changes are handed to our thread.

use super::{update, AudioState, Output};
use std::{
    ffi::c_void,
    sync::{mpsc, Mutex, OnceLock},
    thread,
};
use tauri::{AppHandle, Manager};
use windows::{
    core::{implement, w, BSTR, GUID, PCWSTR},
    Win32::{
        Devices::FunctionDiscovery::PKEY_Device_FriendlyName,
        Foundation::{HANDLE, PROPERTYKEY},
        Media::Audio::{
            eCommunications, eConsole, eMultimedia, eRender,
            Endpoints::{IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl},
            EDataFlow, ERole, IMMDevice, IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl,
            MMDeviceEnumerator, AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE, DEVICE_STATE_ACTIVE,
        },
        System::{
            Com::{
                CoCreateInstance, CoInitializeEx, CoSetProxyBlanket, CoTaskMemFree, CLSCTX_ALL, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
                EOAC_NONE, RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE, STGM_READ,
            },
            Rpc::{RPC_C_AUTHN_WINNT, RPC_C_AUTHZ_NONE},
            Power::{PowerSettingRegisterNotification, DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS, POWERBROADCAST_SETTING},
            SystemServices::GUID_VIDEO_CURRENT_MONITOR_BRIGHTNESS,
            Variant::VARIANT,
            Wmi::{IWbemClassObject, IWbemLocator, IWbemServices, WbemLocator, WBEM_FLAG_FORWARD_ONLY, WBEM_FLAG_RETURN_IMMEDIATELY, WBEM_INFINITE},
        },
        UI::WindowsAndMessaging::REGISTER_NOTIFICATION_FLAGS,
    },
};

pub enum Cmd {
    Volume(u32),
    Mute(bool),
    Brightness(u32),
    Output(String),
    /// The default output changed: listen to the new one.
    Rebind,
    /// Outputs were added, removed, enabled or disabled.
    Devices,
}

static TX: OnceLock<Mutex<mpsc::Sender<Cmd>>> = OnceLock::new();
static APP: OnceLock<AppHandle> = OnceLock::new();

pub fn send(cmd: Cmd) -> Result<(), String> {
    let tx = TX.get().ok_or("El audio todavía no está listo")?;
    tx.lock().unwrap().send(cmd).map_err(|_| "El audio no responde".to_string())
}

pub fn start(app: AppHandle) {
    let _ = APP.set(app);
    let (tx, rx) = mpsc::channel();
    let _ = TX.set(Mutex::new(tx));
    thread::spawn(move || unsafe { run(rx) });
}

fn app() -> Option<&'static AppHandle> {
    APP.get()
}

// ---- Volume -------------------------------------------------------------

#[implement(IAudioEndpointVolumeCallback)]
struct VolumeEvents;

impl IAudioEndpointVolumeCallback_Impl for VolumeEvents_Impl {
    fn OnNotify(&self, data: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows::core::Result<()> {
        let (Some(app), Some(d)) = (app(), unsafe { data.as_ref() }) else { return Ok(()) };
        let volume = (d.fMasterVolume * 100.0).round().clamp(0.0, 100.0) as u32;
        let muted = d.bMuted.as_bool();
        let was_muted = app.state::<AudioState>().status.lock().unwrap().muted;
        update(app, Some(if muted != was_muted { "mute" } else { "volume" }), |s| {
            s.volume = volume;
            s.muted = muted;
        });
        Ok(())
    }
}

#[implement(IMMNotificationClient)]
struct DeviceEvents;

impl IMMNotificationClient_Impl for DeviceEvents_Impl {
    fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> windows::core::Result<()> {
        let _ = send(Cmd::Devices);
        Ok(())
    }
    fn OnDeviceAdded(&self, _: &PCWSTR) -> windows::core::Result<()> {
        let _ = send(Cmd::Devices);
        Ok(())
    }
    fn OnDeviceRemoved(&self, _: &PCWSTR) -> windows::core::Result<()> {
        let _ = send(Cmd::Devices);
        Ok(())
    }
    fn OnDefaultDeviceChanged(&self, flow: EDataFlow, role: ERole, _: &PCWSTR) -> windows::core::Result<()> {
        if flow == eRender && role == eConsole {
            let _ = send(Cmd::Rebind);
        }
        Ok(())
    }
    fn OnPropertyValueChanged(&self, _: &PCWSTR, _: &PROPERTYKEY) -> windows::core::Result<()> {
        Ok(())
    }
}

unsafe fn id_of(device: &IMMDevice) -> Option<String> {
    let p = device.GetId().ok()?;
    let id = p.to_string().ok();
    CoTaskMemFree(Some(p.0 as *const c_void));
    id
}

unsafe fn name_of(device: &IMMDevice) -> String {
    device
        .OpenPropertyStore(STGM_READ)
        .and_then(|store| store.GetValue(&PKEY_Device_FriendlyName))
        .map(|v| v.to_string())
        .unwrap_or_default()
}

/// The endpoint we listen to (the default output) and its callback.
struct Bound {
    endpoint: IAudioEndpointVolume,
    id: String,
}

/// Listen to the current default output. `cause` = "output" when it changed.
unsafe fn bind(en: &IMMDeviceEnumerator, cb: &IAudioEndpointVolumeCallback, old: Option<Bound>, cause: Option<&'static str>) -> Option<Bound> {
    if let Some(o) = old {
        let _ = o.endpoint.UnregisterControlChangeNotify(cb);
    }
    let app = app()?;
    let device = match en.GetDefaultAudioEndpoint(eRender, eConsole) {
        Ok(d) => d,
        Err(_) => {
            update(app, None, |s| {
                s.available = false;
                s.output.clear();
                s.outputs.iter_mut().for_each(|o| o.default = false);
            });
            return None;
        }
    };
    let endpoint: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None).ok()?;
    let _ = endpoint.RegisterControlChangeNotify(cb);
    let volume = endpoint.GetMasterVolumeLevelScalar().map(|v| (v * 100.0).round() as u32).unwrap_or(0);
    let muted = endpoint.GetMute().map(|m| m.as_bool()).unwrap_or(false);
    let id = id_of(&device).unwrap_or_default();
    let name = name_of(&device);
    update(app, cause, |s| {
        s.available = true;
        s.volume = volume.min(100);
        s.muted = muted;
        s.output = name;
        s.outputs.iter_mut().for_each(|o| o.default = o.id == id);
    });
    Some(Bound { endpoint, id })
}

unsafe fn refresh_outputs(en: &IMMDeviceEnumerator, default_id: &str) {
    let Some(app) = app() else { return };
    let Ok(list) = en.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) else { return };
    let n = list.GetCount().unwrap_or(0);
    let mut outputs = Vec::new();
    for i in 0..n {
        if let Ok(d) = list.Item(i) {
            if let Some(id) = id_of(&d) {
                outputs.push(Output { default: id == default_id, name: name_of(&d), id });
            }
        }
    }
    outputs.sort_by_key(|o| o.name.to_lowercase());
    update(app, None, |s| s.outputs = outputs);
}

// Not in the SDK: the interface Windows' own sound settings use to change the
// default device (the same one EarTrumpet and SoundSwitch use). Only the
// position of SetDefaultEndpoint in the vtable matters; the methods before it
// are never called.
#[allow(non_snake_case)]
mod policy {
    use windows::core::{interface, IUnknown, IUnknown_Vtbl, HRESULT, PCWSTR};
    use windows::Win32::Media::Audio::ERole;

    #[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
    pub unsafe trait IPolicyConfig: IUnknown {
        fn GetMixFormat(&self) -> HRESULT;
        fn GetDeviceFormat(&self) -> HRESULT;
        fn ResetDeviceFormat(&self) -> HRESULT;
        fn SetDeviceFormat(&self) -> HRESULT;
        fn GetProcessingPeriod(&self) -> HRESULT;
        fn SetProcessingPeriod(&self) -> HRESULT;
        fn GetShareMode(&self) -> HRESULT;
        fn SetShareMode(&self) -> HRESULT;
        fn GetPropertyValue(&self) -> HRESULT;
        fn SetPropertyValue(&self) -> HRESULT;
        fn SetDefaultEndpoint(&self, id: PCWSTR, role: ERole) -> HRESULT;
    }

    /// The generated methods are private to this module.
    pub unsafe fn set_default(pc: &IPolicyConfig, id: PCWSTR, role: ERole) -> HRESULT {
        pc.SetDefaultEndpoint(id, role)
    }
}
use policy::IPolicyConfig;

const CLSID_POLICY_CONFIG: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

unsafe fn set_default_output(id: &str) {
    // Only ids we listed: never hand Windows an arbitrary string.
    let known = app().is_some_and(|a| a.state::<AudioState>().status.lock().unwrap().outputs.iter().any(|o| o.id == id));
    if !known {
        return;
    }
    let Ok(pc) = CoCreateInstance::<_, IPolicyConfig>(&CLSID_POLICY_CONFIG, None, CLSCTX_ALL) else { return };
    let wide: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
    for role in [eConsole, eMultimedia, eCommunications] {
        let _ = policy::set_default(&pc, PCWSTR(wide.as_ptr()), role);
    }
}

// ---- Brightness ---------------------------------------------------------

/// WMI handles for the laptop panel's brightness. None on desktops and on
/// external monitors, whose brightness the PC doesn't control.
struct Panel {
    svc: IWbemServices,
    path: BSTR,
    params: IWbemClassObject,
}

unsafe fn first(svc: &IWbemServices, query: &str) -> Option<IWbemClassObject> {
    let en = svc
        .ExecQuery(&BSTR::from("WQL"), &BSTR::from(query), WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY, None)
        .ok()?;
    let mut objs = [None];
    let mut n = 0u32;
    let _ = en.Next(WBEM_INFINITE, &mut objs, &mut n);
    objs[0].take()
}

impl Panel {
    unsafe fn connect() -> Result<Panel, String> {
        fn e(step: &'static str) -> impl Fn(windows::core::Error) -> String {
            move |err| format!("{step}: {err}")
        }
        let loc: IWbemLocator = CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER).map_err(e("locator"))?;
        let svc = loc
            .ConnectServer(&BSTR::from(r"ROOT\WMI"), &BSTR::new(), &BSTR::new(), &BSTR::new(), 0, &BSTR::new(), None)
            .map_err(e("connect"))?;
        // WMI calls need impersonation; the process default may not allow it.
        CoSetProxyBlanket(&svc, RPC_C_AUTHN_WINNT, RPC_C_AUTHZ_NONE, None, RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE, None, EOAC_NONE)
            .map_err(e("blanket"))?;
        let methods = first(&svc, "SELECT * FROM WmiMonitorBrightnessMethods WHERE Active = TRUE").ok_or("no methods instance")?;
        let mut path = VARIANT::default();
        methods.Get(w!("__PATH"), 0, &mut path, None, None).map_err(e("__PATH"))?;
        let path = BSTR::try_from(&path).map_err(e("path"))?;
        let mut class = None;
        svc.GetObject(&BSTR::from("WmiMonitorBrightnessMethods"), Default::default(), None, Some(&mut class), None)
            .map_err(e("class"))?;
        let mut input = None;
        class.ok_or("no class")?.GetMethod(w!("WmiSetBrightness"), 0, &mut input, std::ptr::null_mut()).map_err(e("method"))?;
        let params = input.ok_or("no input")?.SpawnInstance(0).map_err(e("spawn"))?;
        let _ = params.Put(w!("Timeout"), 0, &VARIANT::from(1i32), 0);
        Ok(Panel { svc, path, params })
    }

    unsafe fn get(&self) -> Option<u32> {
        let o = first(&self.svc, "SELECT CurrentBrightness FROM WmiMonitorBrightness WHERE Active = TRUE")?;
        let mut v = VARIANT::default();
        o.Get(w!("CurrentBrightness"), 0, &mut v, None, None).ok()?;
        i32::try_from(&v).ok().map(|n| n.clamp(0, 100) as u32)
    }

    unsafe fn set(&self, level: u32) {
        let _ = self.params.Put(w!("Brightness"), 0, &VARIANT::from(level.min(100) as u8), 0);
        let _ = self.svc.ExecMethod(&self.path, &BSTR::from("WmiSetBrightness"), Default::default(), None, &self.params, None, None);
    }
}

/// Windows reports every brightness change (keys, Action Center, power plan).
unsafe extern "system" fn on_brightness(_: *const c_void, kind: u32, setting: *const c_void) -> u32 {
    const PBT_POWERSETTINGCHANGE: u32 = 0x8013;
    let s = setting as *const POWERBROADCAST_SETTING;
    if kind == PBT_POWERSETTINGCHANGE && !s.is_null() && (*s).PowerSetting == GUID_VIDEO_CURRENT_MONITOR_BRIGHTNESS && (*s).DataLength >= 4 {
        let level = std::ptr::read_unaligned((*s).Data.as_ptr() as *const u32).min(100);
        if let Some(app) = app() {
            update(app, Some("brightness"), |st| st.brightness = Some(level));
        }
    }
    0
}

unsafe fn watch_brightness() {
    const DEVICE_NOTIFY_CALLBACK: REGISTER_NOTIFICATION_FLAGS = REGISTER_NOTIFICATION_FLAGS(2);
    // Lives as long as the registration (the whole run).
    let params: &'static mut DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS =
        Box::leak(Box::new(DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS { Callback: Some(on_brightness), Context: std::ptr::null_mut() }));
    let mut handle = std::ptr::null_mut();
    let _ = PowerSettingRegisterNotification(
        &GUID_VIDEO_CURRENT_MONITOR_BRIGHTNESS,
        DEVICE_NOTIFY_CALLBACK,
        HANDLE(params as *mut _ as *mut c_void),
        &mut handle,
    );
}

// ---- The thread ---------------------------------------------------------

unsafe fn run(rx: mpsc::Receiver<Cmd>) {
    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    let Ok(en) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) else { return };
    let devices: IMMNotificationClient = DeviceEvents.into();
    let _ = en.RegisterEndpointNotificationCallback(&devices);
    let cb: IAudioEndpointVolumeCallback = VolumeEvents.into();

    let mut bound = bind(&en, &cb, None, None);
    refresh_outputs(&en, bound.as_ref().map_or("", |b| b.id.as_str()));

    // Known level first, so registering (which reports the current level) shows nothing.
    let panel = Panel::connect().ok();
    if let (Some(p), Some(app)) = (&panel, app()) {
        let level = p.get();
        update(app, None, |s| s.brightness = level);
        if level.is_some() {
            watch_brightness();
        }
    }

    while let Ok(first) = rx.recv() {
        // A slider drag sends many values: only the last one matters.
        let mut cmds = vec![first];
        cmds.extend(rx.try_iter());
        let (mut volume, mut brightness) = (None, None);
        for cmd in cmds {
            match cmd {
                Cmd::Volume(v) => volume = Some(v),
                Cmd::Brightness(v) => brightness = Some(v),
                Cmd::Mute(m) => {
                    if let Some(b) = &bound {
                        let _ = b.endpoint.SetMute(m, std::ptr::null());
                    }
                }
                Cmd::Output(id) => set_default_output(&id),
                Cmd::Rebind => {
                    bound = bind(&en, &cb, bound.take(), Some("output"));
                    refresh_outputs(&en, bound.as_ref().map_or("", |b| b.id.as_str()));
                }
                Cmd::Devices => refresh_outputs(&en, bound.as_ref().map_or("", |b| b.id.as_str())),
            }
        }
        if let (Some(v), Some(b)) = (volume, &bound) {
            let _ = b.endpoint.SetMasterVolumeLevelScalar(v as f32 / 100.0, std::ptr::null());
        }
        if let (Some(v), Some(p)) = (brightness, &panel) {
            p.set(v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs a laptop panel: `cargo test -p ibrae brightness -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn brightness_via_wmi() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let p = Panel::connect().unwrap();
            println!("brightness = {:?}", p.get());
        }
    }
}
