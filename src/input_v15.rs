use anyhow::{Context, Result};
use rdev::{Event, EventType, Key};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputSignal {
    StartProfile(String),
    StopProfile,
    ToggleHandsFree,
    StartVoiceEdit,
    StopVoiceEdit,
}

pub type InputTx = mpsc::Sender<InputSignal>;
pub type InputRx = mpsc::Receiver<InputSignal>;
const DEBOUNCE_MS: u64 = 200;

#[derive(Debug, Clone)]
pub struct Hotkey {
    pub modifiers: HashSet<Modifier>,
    pub trigger: Key,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modifier { Ctrl, Shift, Alt, Meta }

#[derive(Debug, Clone)]
pub struct HotkeySet {
    pub profiles: Vec<(Hotkey, String)>,
    pub hands_free: Option<Hotkey>,
    pub voice_edit: Option<Hotkey>,
}

pub struct SharedHotkeys { bindings: RwLock<HotkeySet> }
impl SharedHotkeys {
    pub fn new(bindings: HotkeySet) -> Arc<Self> { Arc::new(Self { bindings: RwLock::new(bindings) }) }
    pub fn update(&self, bindings: HotkeySet) {
        let count = bindings.profiles.len();
        *self.bindings.write().expect("SharedHotkeys lock poisoned") = bindings;
        info!(count, "Hotkey bindings updated");
    }
    fn read(&self) -> std::sync::RwLockReadGuard<'_, HotkeySet> {
        self.bindings.read().expect("SharedHotkeys lock poisoned")
    }
}

pub fn parse_hotkey(raw: &str) -> Result<Hotkey> {
    let parts: Vec<String> = raw.split('+').map(|v| v.trim().to_lowercase()).filter(|v| !v.is_empty()).collect();
    if parts.is_empty() { anyhow::bail!("Hotkey string is empty"); }
    let mut modifiers = HashSet::new();
    let mut trigger = None;
    for part in &parts {
        match part.as_str() {
            "ctrl" | "control" => { modifiers.insert(Modifier::Ctrl); }
            "shift" => { modifiers.insert(Modifier::Shift); }
            "alt" | "option" => { modifiers.insert(Modifier::Alt); }
            "meta" | "super" | "win" | "cmd" | "command" => { modifiers.insert(Modifier::Meta); }
            _ => {
                if trigger.is_some() { anyhow::bail!("Multiple trigger keys in '{raw}'"); }
                trigger = Some(str_to_key(part).with_context(|| format!("Unknown key '{part}'"))?);
            }
        }
    }
    Ok(Hotkey { modifiers, trigger: trigger.context("Missing trigger key")?, label: raw.to_string() })
}

fn str_to_key(name: &str) -> Result<Key> {
    Ok(match name {
        "a"=>Key::KeyA,"b"=>Key::KeyB,"c"=>Key::KeyC,"d"=>Key::KeyD,"e"=>Key::KeyE,"f"=>Key::KeyF,"g"=>Key::KeyG,"h"=>Key::KeyH,"i"=>Key::KeyI,"j"=>Key::KeyJ,"k"=>Key::KeyK,"l"=>Key::KeyL,"m"=>Key::KeyM,"n"=>Key::KeyN,"o"=>Key::KeyO,"p"=>Key::KeyP,"q"=>Key::KeyQ,"r"=>Key::KeyR,"s"=>Key::KeyS,"t"=>Key::KeyT,"u"=>Key::KeyU,"v"=>Key::KeyV,"w"=>Key::KeyW,"x"=>Key::KeyX,"y"=>Key::KeyY,"z"=>Key::KeyZ,
        "0"=>Key::Num0,"1"=>Key::Num1,"2"=>Key::Num2,"3"=>Key::Num3,"4"=>Key::Num4,"5"=>Key::Num5,"6"=>Key::Num6,"7"=>Key::Num7,"8"=>Key::Num8,"9"=>Key::Num9,
        "f1"=>Key::F1,"f2"=>Key::F2,"f3"=>Key::F3,"f4"=>Key::F4,"f5"=>Key::F5,"f6"=>Key::F6,"f7"=>Key::F7,"f8"=>Key::F8,"f9"=>Key::F9,"f10"=>Key::F10,"f11"=>Key::F11,"f12"=>Key::F12,
        "space"|"spacebar"=>Key::Space,"enter"|"return"=>Key::Return,"tab"=>Key::Tab,"escape"|"esc"=>Key::Escape,"backspace"=>Key::Backspace,"delete"|"del"=>Key::Delete,"insert"|"ins"=>Key::Insert,"home"=>Key::Home,"end"=>Key::End,"pageup"|"pgup"=>Key::PageUp,"pagedown"|"pgdn"|"pgdown"=>Key::PageDown,"up"=>Key::UpArrow,"down"=>Key::DownArrow,"left"=>Key::LeftArrow,"right"=>Key::RightArrow,"capslock"|"caps"=>Key::CapsLock,"printscreen"|"prtsc"=>Key::PrintScreen,"scrolllock"=>Key::ScrollLock,"pause"=>Key::Pause,"`"|"grave"|"backtick"=>Key::BackQuote,"-"|"minus"=>Key::Minus,"="|"equal"|"equals"=>Key::Equal,"["|"bracketleft"=>Key::LeftBracket,"]"|"bracketright"=>Key::RightBracket,"\\"|"backslash"=>Key::BackSlash,";"|"semicolon"=>Key::SemiColon,"'"|"quote"|"apostrophe"=>Key::Quote,","|"comma"=>Key::Comma,"."|"period"|"dot"=>Key::Dot,"/"|"slash"=>Key::Slash,
        _ => anyhow::bail!("Unknown key '{name}'"),
    })
}

#[derive(Clone)]
enum HoldAction { Profile(String), VoiceEdit }
struct HookState {
    modifiers: HashSet<Modifier>,
    triggers: HashSet<Key>,
    active_hold: Option<HoldAction>,
    hands_free_active: bool,
    last_trigger: Instant,
    shared: Arc<SharedHotkeys>,
    tx: InputTx,
}
impl HookState {
    fn new(tx: InputTx, shared: Arc<SharedHotkeys>) -> Self {
        Self { modifiers: HashSet::new(), triggers: HashSet::new(), active_hold: None, hands_free_active: false, last_trigger: Instant::now()-std::time::Duration::from_secs(10), shared, tx }
    }
    fn matches(&self, h: &Hotkey) -> bool { h.modifiers.iter().all(|m| self.modifiers.contains(m)) && self.triggers.contains(&h.trigger) }
    fn debounced(&mut self) -> bool { let now=Instant::now(); if now.duration_since(self.last_trigger).as_millis()<DEBOUNCE_MS as u128 { true } else { self.last_trigger=now; false } }
    fn handle(&mut self, event: &Event) {
        match event.event_type {
            EventType::KeyPress(k) => {
                let changed=if let Some(m)=key_to_modifier(k){self.modifiers.insert(m)}else{self.triggers.insert(k)};
                if changed { self.check_combo(); }
            }
            EventType::KeyRelease(k) => {
                if let Some(m)=key_to_modifier(k){self.modifiers.remove(&m);}else{self.triggers.remove(&k);}
                self.check_release();
            }
            _=>{}
        }
    }
    fn check_combo(&mut self) {
        if self.active_hold.is_some(){return;}
        let bindings=self.shared.read().clone();
        if bindings.hands_free.as_ref().is_some_and(|h|self.matches(h)) {
            if self.debounced(){return;}
            self.hands_free_active=!self.hands_free_active;
            let _=self.tx.blocking_send(InputSignal::ToggleHandsFree);
            return;
        }
        if self.hands_free_active{return;}
        if bindings.voice_edit.as_ref().is_some_and(|h|self.matches(h)) {
            if self.debounced(){return;}
            self.active_hold=Some(HoldAction::VoiceEdit);
            let _=self.tx.blocking_send(InputSignal::StartVoiceEdit);
            return;
        }
        let mut best:Option<&(Hotkey,String)>=None;
        for item in &bindings.profiles {
            if self.matches(&item.0) && best.as_ref().is_none_or(|b|item.0.modifiers.len()>b.0.modifiers.len()) { best=Some(item); }
        }
        if let Some((_,name))=best {
            if self.debounced(){return;}
            self.active_hold=Some(HoldAction::Profile(name.clone()));
            let _=self.tx.blocking_send(InputSignal::StartProfile(name.clone()));
        }
    }
    fn check_release(&mut self) {
        let Some(active)=self.active_hold.clone() else{return;};
        let bindings=self.shared.read().clone();
        let held=match &active {
            HoldAction::Profile(name)=>bindings.profiles.iter().find(|(_,n)|n==name).is_some_and(|(h,_)|self.matches(h)),
            HoldAction::VoiceEdit=>bindings.voice_edit.as_ref().is_some_and(|h|self.matches(h)),
        };
        if held{return;}
        self.active_hold=None;
        let signal=match active {HoldAction::Profile(_)=>InputSignal::StopProfile,HoldAction::VoiceEdit=>InputSignal::StopVoiceEdit};
        let _=self.tx.blocking_send(signal);
    }
}
fn key_to_modifier(k:Key)->Option<Modifier>{match k{Key::ControlLeft|Key::ControlRight=>Some(Modifier::Ctrl),Key::ShiftLeft|Key::ShiftRight=>Some(Modifier::Shift),Key::Alt|Key::AltGr=>Some(Modifier::Alt),Key::MetaLeft|Key::MetaRight=>Some(Modifier::Meta),_=>None}}

pub fn spawn_listener(tx:InputTx,shutdown:Arc<AtomicBool>,shared:Arc<SharedHotkeys>)->Result<std::thread::JoinHandle<()>>{
    #[cfg(target_os="linux")]
    if is_wayland(){return spawn_evdev_listener(tx,shutdown,shared);}
    spawn_rdev_listener(tx,shutdown,shared)
}
pub fn is_wayland()->bool{std::env::var("XDG_SESSION_TYPE").map(|v|v.eq_ignore_ascii_case("wayland")).unwrap_or(false)}
fn spawn_rdev_listener(tx:InputTx,shutdown:Arc<AtomicBool>,shared:Arc<SharedHotkeys>)->Result<std::thread::JoinHandle<()>>{
    std::thread::Builder::new().name("g-type-input".into()).spawn(move||{
        let state=Arc::new(std::sync::Mutex::new(HookState::new(tx,shared)));
        let callback=move|event:Event|{if !shutdown.load(Ordering::Relaxed){if let Ok(mut s)=state.lock(){s.handle(&event);}}};
        if let Err(err)=rdev::listen(callback){error!(?err,"Global keyboard listener crashed");}
    }).context("Failed to spawn input listener thread")
}

#[cfg(target_os="linux")]
#[repr(C)]#[derive(Clone,Copy)]struct LinuxInputEvent{time:libc::timeval,event_type:u16,code:u16,value:i32}
#[cfg(target_os="linux")]
fn spawn_evdev_listener(tx:InputTx,shutdown:Arc<AtomicBool>,shared:Arc<SharedHotkeys>)->Result<std::thread::JoinHandle<()>>{
    std::thread::Builder::new().name("g-type-input-evdev".into()).spawn(move||{
        let state=Arc::new(std::sync::Mutex::new(HookState::new(tx,shared)));
        let mut readers=Vec::new();
        for path in find_keyboard_devices(){if let Ok(file)=std::fs::OpenOptions::new().read(true).open(&path){if set_nonblocking(&file).is_ok(){let state=state.clone();let shutdown=shutdown.clone();readers.push(std::thread::spawn(move||evdev_loop(file,state,shutdown)));}}}
        if readers.is_empty(){warn!("Wayland evdev unavailable; falling back to rdev/XWayland");let state=state.clone();let cb=move|e:Event|{if !shutdown.load(Ordering::Relaxed){if let Ok(mut s)=state.lock(){s.handle(&e);}}};let _=rdev::listen(cb);return;}
        info!(devices=readers.len(),"Wayland evdev keyboard listener started");for r in readers{let _=r.join();}
    }).context("Failed to spawn evdev listener thread")
}
#[cfg(target_os="linux")]
fn set_nonblocking(file:&std::fs::File)->std::io::Result<()>{use std::os::fd::AsRawFd;let fd=file.as_raw_fd();let flags=unsafe{libc::fcntl(fd,libc::F_GETFL)};if flags<0{return Err(std::io::Error::last_os_error());}if unsafe{libc::fcntl(fd,libc::F_SETFL,flags|libc::O_NONBLOCK)}<0{return Err(std::io::Error::last_os_error());}Ok(())}
#[cfg(target_os="linux")]
fn evdev_loop(file:std::fs::File,state:Arc<std::sync::Mutex<HookState>>,shutdown:Arc<AtomicBool>){use std::os::fd::AsRawFd;let fd=file.as_raw_fd();while !shutdown.load(Ordering::Relaxed){let mut e=std::mem::MaybeUninit::<LinuxInputEvent>::uninit();let size=std::mem::size_of::<LinuxInputEvent>();let n=unsafe{libc::read(fd,e.as_mut_ptr().cast(),size)};if n==size as isize{let e=unsafe{e.assume_init()};if e.event_type==1&&e.value!=2{if let Some(k)=linux_keycode(e.code){let t=if e.value==0{EventType::KeyRelease(k)}else{EventType::KeyPress(k)};let event=Event{time:std::time::SystemTime::now(),name:None,event_type:t};if let Ok(mut s)=state.lock(){s.handle(&event);}}}}else{std::thread::sleep(std::time::Duration::from_millis(5));}}}
#[cfg(target_os="linux")]
fn linux_keycode(c:u16)->Option<Key>{Some(match c{1=>Key::Escape,2=>Key::Num1,3=>Key::Num2,4=>Key::Num3,5=>Key::Num4,6=>Key::Num5,7=>Key::Num6,8=>Key::Num7,9=>Key::Num8,10=>Key::Num9,11=>Key::Num0,12=>Key::Minus,13=>Key::Equal,14=>Key::Backspace,15=>Key::Tab,16=>Key::KeyQ,17=>Key::KeyW,18=>Key::KeyE,19=>Key::KeyR,20=>Key::KeyT,21=>Key::KeyY,22=>Key::KeyU,23=>Key::KeyI,24=>Key::KeyO,25=>Key::KeyP,26=>Key::LeftBracket,27=>Key::RightBracket,28=>Key::Return,29=>Key::ControlLeft,30=>Key::KeyA,31=>Key::KeyS,32=>Key::KeyD,33=>Key::KeyF,34=>Key::KeyG,35=>Key::KeyH,36=>Key::KeyJ,37=>Key::KeyK,38=>Key::KeyL,39=>Key::SemiColon,40=>Key::Quote,41=>Key::BackQuote,42=>Key::ShiftLeft,43=>Key::BackSlash,44=>Key::KeyZ,45=>Key::KeyX,46=>Key::KeyC,47=>Key::KeyV,48=>Key::KeyB,49=>Key::KeyN,50=>Key::KeyM,51=>Key::Comma,52=>Key::Dot,53=>Key::Slash,54=>Key::ShiftRight,56=>Key::Alt,57=>Key::Space,58=>Key::CapsLock,59=>Key::F1,60=>Key::F2,61=>Key::F3,62=>Key::F4,63=>Key::F5,64=>Key::F6,65=>Key::F7,66=>Key::F8,67=>Key::F9,68=>Key::F10,70=>Key::ScrollLock,87=>Key::F11,88=>Key::F12,97=>Key::ControlRight,99=>Key::PrintScreen,100=>Key::AltGr,102=>Key::Home,103=>Key::UpArrow,104=>Key::PageUp,105=>Key::LeftArrow,106=>Key::RightArrow,107=>Key::End,108=>Key::DownArrow,109=>Key::PageDown,110=>Key::Insert,111=>Key::Delete,119=>Key::Pause,125=>Key::MetaLeft,126=>Key::MetaRight,_=>return None})}
#[cfg(target_os="linux")]
fn find_keyboard_devices()->Vec<std::path::PathBuf>{let mut out=Vec::new();if let Ok(entries)=std::fs::read_dir("/dev/input/"){for e in entries.flatten(){let p=e.path();let Some(n)=p.file_name().and_then(|n|n.to_str()) else{continue};if !n.starts_with("event"){continue}let cap=format!("/sys/class/input/{n}/device/capabilities/ev");if let Ok(v)=std::fs::read_to_string(cap){if u64::from_str_radix(v.trim(),16).is_ok_and(|x|x&(1<<1)!=0){out.push(p);}}}}out}

#[cfg(test)]mod tests{use super::*;#[test]fn parses_controls(){assert_eq!(parse_hotkey("ctrl+shift+h").unwrap().trigger,Key::KeyH);assert_eq!(parse_hotkey("ctrl+shift+e").unwrap().trigger,Key::KeyE);}#[cfg(target_os="linux")][test]fn linux_codes(){assert_eq!(linux_keycode(35),Some(Key::KeyH));assert_eq!(linux_keycode(18),Some(Key::KeyE));}}
