// app.rs — Finite State Machine orchestrating the G-Type daemon.
// One recording pipeline serves push-to-talk, hands-free and Voice Edit.

#[path = "recovery.rs"]
pub(crate) mod recovery;
#[path = "context.rs"]
pub(crate) mod context;
#[path = "prompt.rs"]
mod prompt;
#[path = "snippets.rs"]
pub(crate) mod snippets;
#[path = "selection.rs"]
mod selection;

use anyhow::{Context, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};
use winit::event_loop::EventLoopProxy;

use crate::audio;
use crate::config::{ConfigV2, Profile};
use crate::injector;
use crate::input::{self, InputRx, InputSignal, InputTx};
use crate::providers;
use crate::transforms;
use crate::ui_bridge::{DaemonEvent, DaemonState, ProfileInfo, UiCommand};

const MAX_RECORDING_SECS: u64 = 10 * 60;
const AUDIO_STARTUP_TIMEOUT_MS: u64 = 1_500;
const VOICE_EDIT_RELEASE_SETTLE_MS: u64 = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Operation {
    Dictation,
    HandsFree,
    VoiceEdit,
}

impl Operation {
    fn as_str(self) -> &'static str {
        match self {
            Self::Dictation => "dictation",
            Self::HandsFree => "hands_free",
            Self::VoiceEdit => "voice_edit",
        }
    }
}

pub async fn run_with_ui(
    config: Arc<RwLock<ConfigV2>>,
    ui_proxy: EventLoopProxy<DaemonEvent>,
    ui_rx: std::sync::mpsc::Receiver<UiCommand>,
) -> Result<()> {
    let shutdown = Arc::new(AtomicBool::new(false));
    let initial_cfg = config.read().await.clone();
    let initial_hotkeys = parsed_hotkeys(&initial_cfg);
    let mut runtime_hotkey_signature = hotkey_signature(&initial_cfg);
    let shared_hotkeys = input::SharedHotkeys::new(initial_hotkeys);
    let _ = crate::config::take_runtime_dirty();
    let (input_tx, mut input_rx): (InputTx, InputRx) = mpsc::channel(32);

    let _input_handle = input::spawn_listener(
        input_tx,
        shutdown.clone(),
        shared_hotkeys.clone(),
    )
    .context("Failed to spawn keyboard listener")?;

    let shutdown_signal = shutdown.clone();
    tokio::spawn(async move {
        let ctrl_c = tokio::signal::ctrl_c();
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sigterm = signal(SignalKind::terminate()).ok();
            tokio::select! {
                _ = ctrl_c => {},
                _ = async {
                    if let Some(ref mut signal) = sigterm { signal.recv().await; }
                    else { std::future::pending::<()>().await; }
                } => {},
            }
        }
        #[cfg(not(unix))]
        {
            let _ = ctrl_c.await;
        }
        info!("Shutdown signal received, cleaning up...");
        shutdown_signal.store(true, Ordering::SeqCst);
    });

    info!(profiles = initial_cfg.profiles.len(), "Ready — hold a mode hotkey to dictate.");
    let _ = ui_proxy.send_event(DaemonEvent::StateChanged(DaemonState::Idle));

    loop {
        if shutdown.load(Ordering::SeqCst) {
            info!("Shutting down gracefully.");
            return Ok(());
        }

        if crate::config::take_runtime_dirty() {
            let cfg = config.read().await.clone();
            let signature = hotkey_signature(&cfg);
            if signature != runtime_hotkey_signature {
                shared_hotkeys.update(parsed_hotkeys(&cfg));
                runtime_hotkey_signature = signature;
                let _ = ui_proxy.send_event(DaemonEvent::ProfilesUpdated(profile_infos(&cfg)));
                info!("Runtime mode/hotkey configuration refreshed");
            }
        }

        while let Ok(command) = ui_rx.try_recv() {
            match command {
                UiCommand::Quit => shutdown.store(true, Ordering::SeqCst),
                UiCommand::OpenSettings => {
                    if let Err(error) = open::that("http://127.0.0.1:9741") {
                        error!(%error, "Failed to open settings");
                    }
                }
                UiCommand::SwitchProfile(name) => info!("Mode selected over UI: {}", name),
            }
        }

        match input_rx.try_recv() {
            Ok(InputSignal::StartProfile(requested)) => {
                start_operation(&config, &mut input_rx, &ui_proxy, Operation::Dictation, requested).await;
            }
            Ok(InputSignal::ToggleHandsFree) => {
                let requested = config.read().await.global.default_profile.clone();
                start_operation(&config, &mut input_rx, &ui_proxy, Operation::HandsFree, requested).await;
            }
            Ok(InputSignal::StartVoiceEdit) => {
                let requested = config.read().await.global.default_profile.clone();
                start_operation(&config, &mut input_rx, &ui_proxy, Operation::VoiceEdit, requested).await;
            }
            Ok(InputSignal::StopProfile | InputSignal::StopVoiceEdit) => continue,
            Err(mpsc::error::TryRecvError::Disconnected) => {
                error!("Input channel closed unexpectedly");
                return Ok(());
            }
            Err(mpsc::error::TryRecvError::Empty) => {
                tokio::time::sleep(std::time::Duration::from_millis(45)).await;
            }
        }
    }
}

fn profile_infos(config: &ConfigV2) -> Vec<ProfileInfo> {
    config.profiles.iter().map(|profile| ProfileInfo {
        name: profile.name.clone(), model_name: profile.model.clone(), active: false,
    }).collect()
}

async fn start_operation(
    config: &Arc<RwLock<ConfigV2>>,
    input_rx: &mut InputRx,
    ui_proxy: &EventLoopProxy<DaemonEvent>,
    operation: Operation,
    requested_profile: String,
) {
    let snapshot = config.read().await.clone();
    let app_context = tokio::task::spawn_blocking(context::capture).await.ok().flatten();
    let Some(profile) = resolve_profile(&snapshot, &requested_profile, app_context.as_ref()) else {
        warn!(requested_profile, "Unknown mode triggered");
        if snapshot.global.sound_enabled { crate::audio_feedback::play_error_beep(); }
        return;
    };

    info!(requested=%requested_profile,effective=%profile.name,operation=operation.as_str(),context=app_context.as_ref().map(|ctx|ctx.id.as_str()).unwrap_or("unavailable"),"🎤 Recording...");
    if snapshot.global.sound_enabled { crate::audio_feedback::play_start_beep(); }
    let _ = ui_proxy.send_event(DaemonEvent::StateChanged(DaemonState::Recording { profile: profile.name.clone() }));
    let _ = ui_proxy.send_event(DaemonEvent::ProfileActivated(ProfileInfo { name: profile.name.clone(), model_name: profile.model.clone(), active: true }));

    state_recording(&snapshot, &profile, operation, app_context, input_rx, ui_proxy).await;
    let _ = ui_proxy.send_event(DaemonEvent::StateChanged(DaemonState::Idle));
}

fn resolve_profile(config:&ConfigV2,requested_profile:&str,app_context:Option<&context::AppContext>)->Option<Profile>{
    let requested=config.profiles.iter().find(|profile|profile.name==requested_profile)?;
    if requested_profile!=config.global.default_profile{return Some(requested.clone());}
    if let Some(app_context)=app_context{if let Some(bound)=config.app_bindings.get(&app_context.id){if let Some(profile)=config.profiles.iter().find(|profile|&profile.name==bound){return Some(profile.clone());}}}
    Some(requested.clone())
}

fn parsed_hotkeys(config:&ConfigV2)->input::HotkeySet{
    let profiles=config.profiles.iter().filter_map(|profile|match input::parse_hotkey(&profile.hotkey){Ok(hotkey)=>Some((hotkey,profile.name.clone())),Err(error)=>{warn!(profile=%profile.name,hotkey=%profile.hotkey,%error,"Ignoring invalid mode hotkey");None}}).collect();
    let hands_free=input::parse_hotkey(&config.global.hands_free_hotkey).map_err(|error|warn!(%error,"Invalid Hands-Free hotkey")).ok();
    let voice_edit=input::parse_hotkey(&config.global.voice_edit_hotkey).map_err(|error|warn!(%error,"Invalid Voice Edit hotkey")).ok();
    input::HotkeySet{profiles,hands_free,voice_edit}
}

fn hotkey_signature(config:&ConfigV2)->Vec<(String,String)>{
    let mut signature:Vec<_>=config.profiles.iter().map(|profile|(profile.name.clone(),profile.hotkey.clone())).collect();
    signature.push(("__hands_free__".into(),config.global.hands_free_hotkey.clone()));
    signature.push(("__voice_edit__".into(),config.global.voice_edit_hotkey.clone()));
    signature
}

async fn state_recording(
    config:&ConfigV2,
    profile:&Profile,
    operation:Operation,
    app_context:Option<context::AppContext>,
    input_rx:&mut InputRx,
    ui_proxy:&EventLoopProxy<DaemonEvent>,
){
    debug!("Capturing audio to buffer");
    let(audio_tx,audio_rx)=audio::audio_channel();let recording_flag=Arc::new(AtomicBool::new(true));let configured_device=config.global.audio_device.clone();
    let audio_thread_handle=match audio::start_capture(audio_tx.clone(),recording_flag.clone(),configured_device.clone()){
        Ok(handle)=>handle,
        Err(first_error) if configured_device.as_deref().is_some_and(|name|!name.is_empty()&&name!="default")=>{warn!(device=configured_device.as_deref().unwrap_or_default(),%first_error,"Configured microphone unavailable; falling back to system default");match audio::start_capture(audio_tx.clone(),recording_flag.clone(),None){Ok(handle)=>handle,Err(error)=>{error!(%error,"Failed to start fallback audio capture");if config.global.sound_enabled{crate::audio_feedback::play_error_beep();}return;}}}
        Err(error)=>{error!(%error,"Failed to start audio capture");if config.global.sound_enabled{crate::audio_feedback::play_error_beep();}return;}
    };drop(audio_tx);
    let first_chunk=match audio_rx.recv_timeout(std::time::Duration::from_millis(AUDIO_STARTUP_TIMEOUT_MS)){Ok(chunk)=>chunk,Err(error)=>{recording_flag.store(false,Ordering::Relaxed);let _=audio_thread_handle.join();error!(%error,"Microphone stream produced no audio; aborting recording early");if config.global.sound_enabled{crate::audio_feedback::play_error_beep();}return;}};
    let collector_handle=tokio::task::spawn_blocking(move||{let mut samples=Vec::<i16>::with_capacity(480_000);samples.extend_from_slice(&first_chunk);while let Ok(chunk)=audio_rx.recv(){samples.extend_from_slice(&chunk);}samples});
    let watchdog=tokio::time::sleep(std::time::Duration::from_secs(MAX_RECORDING_SECS));tokio::pin!(watchdog);let mut can_process=true;
    loop{tokio::select!{signal=input_rx.recv()=>{match(operation,signal){(Operation::Dictation,Some(InputSignal::StopProfile))=>break,(Operation::HandsFree,Some(InputSignal::ToggleHandsFree))=>break,(Operation::VoiceEdit,Some(InputSignal::StopVoiceEdit))=>break,(_,None)=>{error!("Input channel closed during recording");can_process=false;break;},_=>continue}},_=&mut watchdog=>{warn!(max_seconds=MAX_RECORDING_SECS,"Recording watchdog reached; stopping safely");break;}}}
    recording_flag.store(false,Ordering::Relaxed);if let Err(error)=audio_thread_handle.join(){error!("Audio capture thread panicked: {:?}",error);can_process=false;}let all_samples=match collector_handle.await{Ok(samples)=>samples,Err(error)=>{error!(%error,"Audio collector task failed");return;}};if !can_process{return;}
    let duration=all_samples.len()as f64/16_000.0;info!(duration=format!("{duration:.1}s"),operation=operation.as_str(),"⏹ Stopped. Processing...");if config.global.sound_enabled{crate::audio_feedback::play_stop_beep();}if all_samples.is_empty(){warn!("No audio captured, skipping processing");if config.global.sound_enabled{crate::audio_feedback::play_error_beep();}return;}

    let selected_text=if operation==Operation::VoiceEdit{
        tokio::time::sleep(std::time::Duration::from_millis(VOICE_EDIT_RELEASE_SETTLE_MS)).await;
        match tokio::task::spawn_blocking(selection::capture_selected_text).await{Ok(Ok(Some(text)))=>Some(text),Ok(Ok(None))=>{warn!("Voice Edit completed without selected text");if config.global.sound_enabled{crate::audio_feedback::play_error_beep();}return;},Ok(Err(error))=>{warn!(%error,"Voice Edit could not capture selected text");if config.global.sound_enabled{crate::audio_feedback::play_error_beep();}return;},Err(error)=>{warn!(%error,"Voice Edit selection task failed");if config.global.sound_enabled{crate::audio_feedback::play_error_beep();}return;}}
    }else{None};

    let recovery_id=match recovery::persist_with_context(&all_samples,&profile.name,&profile.model,&config.global.language,app_context.as_ref(),Some(operation.as_str()),selected_text.as_deref()){Ok(item)=>Some(item.id),Err(error)=>{error!(%error,"Could not persist recovery audio; continuing");None}};
    let _=ui_proxy.send_event(DaemonEvent::StateChanged(DaemonState::Processing{profile:profile.name.clone()}));
    let request_prompt=match operation{Operation::VoiceEdit=>prompt::build_voice_edit_prompt(&config.global.language,selected_text.as_deref().unwrap_or_default(),app_context.as_ref(),&config.snippets),Operation::Dictation|Operation::HandsFree=>prompt::build_dictation_prompt(&config.global.language,profile,app_context.as_ref(),&config.snippets)};
    let outcome=match providers::transcribe_with_fallback_prompt(profile,&config.keys,&all_samples,&config.global.language,&request_prompt).await{Ok(outcome)=>outcome,Err(error)=>{preserve_failure(recovery_id.as_deref(),&error.to_string());error!(kind=?error.kind,%error,"Gemini processing failed; audio preserved in Recovery");if config.global.sound_enabled{crate::audio_feedback::play_error_beep();}return;}};
    if outcome.text.trim().is_empty(){preserve_failure(recovery_id.as_deref(),"Gemini returned an empty result");warn!("Empty result received; audio preserved in Recovery");return;}
    let final_text=match operation{Operation::VoiceEdit=>outcome.text.trim().to_string(),Operation::Dictation|Operation::HandsFree=>{let transformed=if profile.transforms.is_empty(){outcome.text.clone()}else{transforms::run_pipeline(&profile.transforms,&outcome.text,&config.global.language).await};snippets::apply(&transformed,&config.snippets)}};
    if final_text.trim().is_empty(){preserve_failure(recovery_id.as_deref(),"Post-processing produced empty text");warn!("Post-processing produced empty text; audio preserved in Recovery");return;}
    let record=crate::tracking::build_record_with_context(&outcome.model_used,duration,&outcome.usage,&final_text,Some(&profile.name),app_context.as_ref(),Some(operation.as_str()));let log_line=crate::tracking::format_log_line(&record,&config.global.currency);info!(model=%outcome.model_used,mode=%profile.name,operation=operation.as_str(),"{}",log_line);
    match crate::tracking::append_record(&record){Ok(())=>{if let Some(id)=recovery_id.as_deref(){if let Err(error)=recovery::remove(id){warn!(%error,id,"History saved but recovery cleanup failed");}}},Err(error)=>{preserve_failure(recovery_id.as_deref(),&format!("Tracking save failed: {error}"));warn!(%error,"Failed to save tracking record; recovery audio kept");}}
    if operation==Operation::VoiceEdit{let current_context=tokio::task::spawn_blocking(context::capture).await.ok().flatten();if !same_context(app_context.as_ref(),current_context.as_ref()){warn!("Voice Edit completed but active application changed; result kept in history without injection");if config.global.sound_enabled{crate::audio_feedback::play_error_beep();}return;}}
    let text=final_text.clone();let inject_result=tokio::task::spawn_blocking(move||injector::inject(&text)).await;match inject_result{Ok(Ok(()))=>info!(text=%truncate(&final_text,80),"✅ Injected"),Ok(Err(error))=>{error!(%error,"Text injection failed; result is available in history");if config.global.sound_enabled{crate::audio_feedback::play_error_beep();}},Err(error)=>{error!(%error,"Injection task panicked; result is available in history");if config.global.sound_enabled{crate::audio_feedback::play_error_beep();}}}
}

fn same_context(before:Option<&context::AppContext>,after:Option<&context::AppContext>)->bool{match(before,after){(Some(before),Some(after))=>before.id==after.id,(None,None)=>true,_=>false}}
fn preserve_failure(id:Option<&str>,message:&str){if let Some(id)=id{match recovery::mark_failure(id,message){Ok(())=>warn!(id,"Recording saved for dashboard recovery"),Err(error)=>error!(%error,id,"Failed to update recovery metadata")}}}
fn truncate(value:&str,max_chars:usize)->String{let mut chars=value.chars();let prefix:String=chars.by_ref().take(max_chars).collect();if chars.next().is_some(){format!("{}…",prefix)}else{prefix}}

#[cfg(test)]mod tests{use super::*;fn gmail()->context::AppContext{context::AppContext{id:"web:chrome:gmail".into(),app_name:"Chrome".into(),app_identifier:"chrome".into(),window_title:None,surface:Some("Gmail".into())}}#[test]fn explicit_mode_wins(){let mut config=ConfigV2::default();config.profiles.push(Profile{name:"email".into(),hotkey:"alt+e".into(),..Profile::default()});config.app_bindings.insert("web:chrome:gmail".into(),"email".into());assert_eq!(resolve_profile(&config,"email",Some(&gmail())).unwrap().name,"email");}#[test]fn default_resolves_binding(){let mut config=ConfigV2::default();config.profiles.push(Profile{name:"email".into(),hotkey:"alt+e".into(),..Profile::default()});config.app_bindings.insert("web:chrome:gmail".into(),"email".into());assert_eq!(resolve_profile(&config,"dictation",Some(&gmail())).unwrap().name,"email");}#[test]fn context_change_blocks_edit(){let a=context::AppContext{id:"app:code".into(),app_name:"Code".into(),app_identifier:"code".into(),window_title:None,surface:None};let b=context::AppContext{id:"app:chrome".into(),app_name:"Chrome".into(),app_identifier:"chrome".into(),window_title:None,surface:None};assert!(!same_context(Some(&a),Some(&b)));assert!(same_context(Some(&a),Some(&a)));}#[test]fn truncate_utf8(){assert_eq!(truncate("èèè",2),"èè…");}}
