// recovery.rs — Durable spool for recordings that must survive provider failures.
// Each stopped recording is persisted as a WAV before any network request.

use anyhow::{bail, Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryItem {
    pub id: String,
    pub created_at: String,
    pub profile: String,
    pub model: String,
    pub language: String,
    pub duration_secs: f64,
    pub attempts: u32,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_context: Option<super::context::AppContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_text: Option<String>,
}

fn data_dir() -> Result<PathBuf> {
    let proj = ProjectDirs::from("", "", "g-type").context("Cannot determine G-Type data directory")?;
    Ok(proj.data_dir().join("recovery"))
}
fn wav_path(id: &str) -> Result<PathBuf> { Ok(data_dir()?.join(format!("{id}.wav"))) }
fn meta_path(id: &str) -> Result<PathBuf> { Ok(data_dir()?.join(format!("{id}.json"))) }

pub fn persist(samples:&[i16],profile:&str,model:&str,language:&str)->Result<RecoveryItem>{persist_with_context(samples,profile,model,language,None,None,None)}

pub fn persist_with_context(
    samples:&[i16], profile:&str, model:&str, language:&str,
    app_context:Option<&super::context::AppContext>, operation:Option<&str>, selected_text:Option<&str>,
)->Result<RecoveryItem>{
    if samples.is_empty(){bail!("Cannot persist empty audio");}
    let dir=data_dir()?;fs::create_dir_all(&dir).with_context(||format!("Cannot create recovery directory {}",dir.display()))?;
    let id=make_id();let wav=crate::providers::gemini::encode_wav(samples);atomic_write(&wav_path(&id)?,&wav)?;
    let item=RecoveryItem{id:id.clone(),created_at:now_utc(),profile:profile.to_string(),model:model.to_string(),language:language.to_string(),duration_secs:samples.len()as f64/16_000.0,attempts:0,last_error:None,app_context:app_context.cloned(),operation:operation.map(str::to_string),selected_text:selected_text.map(|text|text.chars().take(20_000).collect())};
    save_item(&item)?;Ok(item)
}

pub fn list()->Result<Vec<RecoveryItem>>{
    let dir=data_dir()?;if !dir.exists(){return Ok(Vec::new());}let mut items=Vec::new();
    for entry in fs::read_dir(&dir).with_context(||format!("Cannot read recovery directory {}",dir.display()))?{let entry=entry?;let path=entry.path();if path.extension().and_then(|ext|ext.to_str())!=Some("json"){continue;}match fs::read_to_string(&path).ok().and_then(|raw|serde_json::from_str::<RecoveryItem>(&raw).ok()){Some(item)if wav_path(&item.id)?.exists()=>items.push(item),Some(_)=>{let _=fs::remove_file(&path);},None=>tracing::warn!(path=%path.display(),"Skipping unreadable recovery metadata")}}
    items.sort_by(|a,b|b.created_at.cmp(&a.created_at));Ok(items)
}

pub fn load(id:&str)->Result<(RecoveryItem,Vec<i16>)>{let item=load_item(id)?;let wav=fs::read(wav_path(id)?).with_context(||format!("Cannot read recovery WAV for {id}"))?;let samples=decode_pcm16_mono_wav(&wav)?;Ok((item,samples))}
pub fn mark_failure(id:&str,error:&str)->Result<()>{let mut item=load_item(id)?;item.attempts=item.attempts.saturating_add(1);item.last_error=Some(error.chars().take(500).collect());save_item(&item)}
pub fn remove(id:&str)->Result<()>{validate_id(id)?;let wav=wav_path(id)?;let meta=meta_path(id)?;if wav.exists(){fs::remove_file(&wav).with_context(||format!("Cannot remove recovery WAV {}",wav.display()))?;}if meta.exists(){fs::remove_file(&meta).with_context(||format!("Cannot remove recovery metadata {}",meta.display()))?;}Ok(())}
pub fn open_audio(id:&str)->Result<()>{validate_id(id)?;let path=wav_path(id)?;if !path.exists(){bail!("Recovery audio not found");}open::that(&path).context("Cannot open recovery audio")?;Ok(())}
fn load_item(id:&str)->Result<RecoveryItem>{validate_id(id)?;let meta=meta_path(id)?;let raw=fs::read_to_string(&meta).with_context(||format!("Cannot read recovery metadata {}",meta.display()))?;serde_json::from_str(&raw).with_context(||format!("Invalid recovery metadata {}",meta.display()))}
fn save_item(item:&RecoveryItem)->Result<()>{let content=serde_json::to_vec_pretty(item).context("Cannot serialize recovery metadata")?;atomic_write(&meta_path(&item.id)?,&content)}
fn atomic_write(path:&Path,bytes:&[u8])->Result<()>{if let Some(parent)=path.parent(){fs::create_dir_all(parent)?;}let tmp=path.with_extension(format!("{}.tmp",path.extension().and_then(|e|e.to_str()).unwrap_or("file")));{let mut file=fs::OpenOptions::new().create(true).truncate(true).write(true).open(&tmp).with_context(||format!("Cannot create temporary file {}",tmp.display()))?;file.write_all(bytes)?;file.sync_all()?;}replace_file(&tmp,path)?;Ok(())}
#[cfg(unix)]fn replace_file(tmp:&Path,destination:&Path)->std::io::Result<()>{fs::rename(tmp,destination)}
#[cfg(not(unix))]fn replace_file(tmp:&Path,destination:&Path)->std::io::Result<()>{if destination.exists(){fs::remove_file(destination)?;}fs::rename(tmp,destination)}
fn validate_id(id:&str)->Result<()>{if id.is_empty()||!id.chars().all(|c|c.is_ascii_alphanumeric()||c=='-'||c=='_'){bail!("Invalid recovery id");}Ok(())}
fn make_id()->String{let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();format!("{}-{}",now.as_secs(),now.subsec_nanos())}
fn now_utc()->String{let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();let secs=now.as_secs();let days=secs/86400;let tod=secs%86400;let hours=tod/3600;let minutes=(tod%3600)/60;let seconds=tod%60;let(year,month,day)=days_to_ymd(days);format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")}
fn days_to_ymd(days:u64)->(i32,u32,u32){let z=days as i64+719468;let era=if z>=0{z}else{z-146096}/146097;let doe=(z-era*146097)as u32;let yoe=(doe-doe/1460+doe/36524-doe/146096)/365;let y=yoe as i64+era*400;let doy=doe-(365*yoe+yoe/4-yoe/100);let mp=(5*doy+2)/153;let d=doy-(153*mp+2)/5+1;let m=if mp<10{mp+3}else{mp-9};let year=if m<=2{y+1}else{y};(year as i32,m,d)}
fn decode_pcm16_mono_wav(bytes:&[u8])->Result<Vec<i16>>{if bytes.len()<44||&bytes[0..4]!=b"RIFF"||&bytes[8..12]!=b"WAVE"{bail!("Invalid recovery WAV header");}let channels=u16::from_le_bytes([bytes[22],bytes[23]]);let bits=u16::from_le_bytes([bytes[34],bytes[35]]);let sample_rate=u32::from_le_bytes([bytes[24],bytes[25],bytes[26],bytes[27]]);if channels!=1||bits!=16||sample_rate!=16_000{bail!("Unsupported recovery WAV format");}let mut offset=12usize;let mut data=None;while offset+8<=bytes.len(){let id=&bytes[offset..offset+4];let len=u32::from_le_bytes([bytes[offset+4],bytes[offset+5],bytes[offset+6],bytes[offset+7]])as usize;let start=offset+8;let end=start.saturating_add(len);if end>bytes.len(){bail!("Corrupted recovery WAV chunk");}if id==b"data"{data=Some(&bytes[start..end]);break;}offset=end+(len%2);}let data=data.context("Recovery WAV has no data chunk")?;if data.len()%2!=0{bail!("Recovery WAV contains incomplete PCM sample");}Ok(data.chunks_exact(2).map(|c|i16::from_le_bytes([c[0],c[1]])).collect())}

#[cfg(test)]mod tests{use super::*;#[test]fn wav_roundtrip(){let samples=vec![0,100,-100,i16::MAX,i16::MIN];let wav=crate::providers::gemini::encode_wav(&samples);assert_eq!(decode_pcm16_mono_wav(&wav).unwrap(),samples);}#[test]fn reject_bad_id(){assert!(validate_id("../oops").is_err());assert!(validate_id("ok-123").is_ok());}#[test]fn old_metadata_is_compatible(){let raw=r#"{"id":"ok-123","created_at":"2026-01-01T00:00:00Z","profile":"dictation","model":"models/gemini-3.5-flash-lite","language":"it","duration_secs":1.0,"attempts":0}"#;let item:RecoveryItem=serde_json::from_str(raw).unwrap();assert!(item.app_context.is_none());assert!(item.operation.is_none());}}
