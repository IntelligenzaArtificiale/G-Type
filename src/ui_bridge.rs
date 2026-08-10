#![allow(dead_code)]

#[derive(Debug, Clone)]
pub enum DaemonEvent {
    StateChanged(DaemonState),
    ProfilesUpdated(Vec<ProfileInfo>),
    ProfileActivated(ProfileInfo),
    Error(String),
    Quit,
}

#[derive(Debug, Clone)]
pub enum DaemonState {
    Idle,
    Recording { profile: String },
    Processing { profile: String },
}

#[derive(Debug, Clone)]
pub struct ProfileInfo {
    pub name: String,
    pub model_name: String,
    pub active: bool,
}

#[derive(Debug, Clone)]
pub enum UiCommand {
    SwitchProfile(String),
    OpenSettings,
    Quit,
}
