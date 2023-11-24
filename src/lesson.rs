use aws_config::SdkConfig;
use aws_sdk_polly::types::{Engine, OutputFormat, SpeechMarkType, VoiceId};
use aws_sdk_transcribestreaming::types::{LanguageCode};
use futures_util::future::try_join;
use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};
use std::io::BufRead;
use std::ops::Deref;
use std::sync::{Arc, Weak};
use tokio::sync::RwLock;
use tracing::{error, warn};

use tokio::select;
use crate::asr::{Event, aws::AwsAsr, ASR};

#[cfg(feature = "whisper")]
use crate::asr::whisper::WhisperAsr;
use crate::Viseme;

pub type LessonID = String;

pub struct InnerLessonsManager {
    translate_client: aws_sdk_translate::Client,
    polly_client: aws_sdk_polly::Client,
    lessons: Arc<RwLock<BTreeMap<LessonID, Lesson>>>,
}

#[derive(Clone)]
pub struct LessonsManager {
    inner: Arc<InnerLessonsManager>,
}

impl Debug for LessonsManager {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LessonsManager").finish()
    }
}

impl Deref for LessonsManager {
    type Target = InnerLessonsManager;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

pub enum AsrEngine {
    AWS,
    #[allow(dead_code)]
    #[cfg(feature = "whisper")]
    Whisper,
}

impl AsrEngine {
    async fn create(self, lang: LanguageCode) -> Box<dyn ASR + Send> {
        match self {
            AsrEngine::AWS => Box::new(
                AwsAsr::from_env(lang)
                    .await
                    .expect("Failed to initialize AWS ASR")
            ),
            #[cfg(feature = "whisper")]
            AsrEngine::Whisper => Box::new(
                WhisperAsr::from_config()
                    .await
                    .expect("Failed to initialize Whisper ASR")
            ),
        }
    }
}

impl LessonsManager {
    pub fn new(sdk_config: &SdkConfig) -> Self {
        let translate_client = aws_sdk_translate::Client::new(sdk_config);
        let polly_client = aws_sdk_polly::Client::new(sdk_config);
        let inner = InnerLessonsManager {
            translate_client,
            polly_client,
            lessons: Arc::new(RwLock::new(BTreeMap::new())),
        };
        LessonsManager { inner: Arc::new(inner) }
    }

    pub(crate) async fn create_lesson(&self, id: LessonID, engine: AsrEngine, speaker_lang: LanguageCode) -> Lesson {
        let mut map = self.lessons.write().await;
        let lesson: Lesson = InnerLesson::new(self.clone(), engine, speaker_lang).await.into();
        map.insert(id, lesson.clone());
        lesson
    }

    pub(crate) async fn get_lesson(&self, id: LessonID) -> Option<Lesson> {
        let map = self.lessons.read().await;
        map.get(&id).cloned()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Lesson {
    inner: Arc<InnerLesson>,
}

impl Deref for Lesson {
    type Target = InnerLesson;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl Lesson {
    pub(crate) async fn get_or_init(&self, lang: String) -> LangLesson {
        {
            let map = self.lang_lessons.read().await;
            if let Some(lang_lesson) = map.get(&lang).and_then(|weak| weak.upgrade()) {
                return lang_lesson.into();
            }
        }
        {
            let mut map = self.lang_lessons.write().await;
            if let Some(lang_lesson) = map.get(&lang).and_then(|weak| weak.upgrade()) {
                lang_lesson.into()
            } else {
                let lang_lesson = LangLesson::new(self.clone(), lang.clone());
                map.insert(lang.clone(), Arc::downgrade(&lang_lesson.inner));
                lang_lesson
            }
        }
    }

    pub(crate) async fn send(&self, frame: Vec<i16>) -> anyhow::Result<()> {
        Ok(self.speaker_voice_channel.send(frame).await?)
    }

    pub(crate) fn transcript_channel(&self) -> tokio::sync::broadcast::Receiver<Event> {
        self.speaker_transcript.subscribe()
    }
}

impl From<InnerLesson> for Lesson {
    fn from(inner: InnerLesson) -> Self {
        Lesson {
            inner: Arc::new(inner),
        }
    }
}

#[derive(Debug)]
pub(crate) struct InnerLesson {
    parent: LessonsManager,
    speaker_lang: LanguageCode,
    speaker_voice_channel: tokio::sync::mpsc::Sender<Vec<i16>>,
    speaker_transcript: tokio::sync::broadcast::Sender<Event>,
    lang_lessons: RwLock<BTreeMap<String, Weak<InnerLangLesson>>>,
    drop_handler: Option<tokio::sync::oneshot::Sender<Signal>>,
}

impl InnerLesson {
    async fn new(parent: LessonsManager, engine: AsrEngine, speaker_lang: LanguageCode) -> InnerLesson {
        let (speaker_transcript, _) = tokio::sync::broadcast::channel::<Event>(128);
        let shared_speaker_transcript = speaker_transcript.clone();
        let (speaker_voice_channel, mut speaker_voice_rx) = tokio::sync::mpsc::channel::<Vec<i16>>(128);
        let (drop_handler, drop_rx) = tokio::sync::oneshot::channel::<Signal>();

        let mut asr: Box<dyn ASR + Send> = engine.create(speaker_lang.clone()).await;

        tokio::spawn(async move {
            let fut = async {
                let mut transcribe = asr.subscribe();
                loop {
                    select! {
                        msg_opt = speaker_voice_rx.recv() => {
                            match msg_opt {
                                Some(frame) => {
                                    asr.frame(frame).await?;
                                },
                                None => break,
                            }
                        },
                        evt_poll = transcribe.recv() => {
                            shared_speaker_transcript.send(evt_poll?)?;
                        }
                    }
                }

                Ok(()) as anyhow::Result<()>
            };
            select! {
                res = fut => {
                    if let Err(e) = res {
                        error!("Error: {:?}", e);
                    }
                }
                _ = drop_rx => {}
            }
        });

        InnerLesson {
            parent,
            speaker_lang,
            speaker_voice_channel,
            speaker_transcript,
            lang_lessons: RwLock::new(BTreeMap::new()),
            drop_handler: Some(drop_handler),
        }
    }
}

impl Drop for InnerLesson {
    fn drop(&mut self) {
        if let Some(tx) = self.drop_handler.take() {
            let _ = tx.send(Signal::Stop);
        }
    }
}

pub(crate) struct InnerLangLesson {
    parent: Lesson,
    translated_tx: tokio::sync::broadcast::Sender<String>,
    voice_lessons: RwLock<BTreeMap<VoiceId, Weak<InnerVoiceLesson>>>,
    drop_handler: Option<tokio::sync::oneshot::Sender<Signal>>,
}

impl Drop for InnerLangLesson {
    fn drop(&mut self) {
        if let Some(tx) = self.drop_handler.take() {
            let _ = tx.send(Signal::Stop);
        }
    }
}

#[derive(Clone)]
pub(crate) struct LangLesson {
    inner: Arc<InnerLangLesson>,
}

impl LangLesson {
    pub(crate) fn translated_channel(&self) -> tokio::sync::broadcast::Receiver<String> {
        self.translated_tx.subscribe()
    }
}

impl Deref for LangLesson {
    type Target = InnerLangLesson;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl From<InnerLangLesson> for LangLesson {
    fn from(inner: InnerLangLesson) -> Self {
        LangLesson {
            inner: Arc::new(inner),
        }
    }
}

impl From<Arc<InnerLangLesson>> for LangLesson {
    fn from(inner: Arc<InnerLangLesson>) -> Self {
        LangLesson { inner }
    }
}

impl LangLesson {
    fn new(parent: Lesson, lang: String) -> Self {
        let shared_lang = lang.clone();
        let shared_speaker_lang = parent.speaker_lang.clone();
        let (translated_tx, _) = tokio::sync::broadcast::channel::<String>(128);
        let shared_translated_tx = translated_tx.clone();
        let mut transcript_rx = parent.speaker_transcript.subscribe();
        let translate_client = parent.parent.translate_client.clone();
        let (drop_handler, drop_rx) = tokio::sync::oneshot::channel::<Signal>();
        tokio::spawn(async move {
            let fut = async {
                while let Ok(evt) = transcript_rx.recv().await {
                    if !evt.is_final { continue }
                    let output = translate_client
                        .translate_text()
                        .text(evt.transcript)
                        .source_language_code(shared_speaker_lang.as_str())
                        .target_language_code(shared_lang.clone())
                        .send()
                        .await;
                    match output {
                        Ok(res) => {
                            let _ = shared_translated_tx.send(res.translated_text);
                        }
                        Err(e) => {
                            return Err(e);
                        }
                    }
                }
                Ok(())
            };

            select! {
                res = fut => {
                    if let Err(e) = res {
                        warn!("Error: {:?}", e);
                    }
                }
                _ = drop_rx => {}
            }
        });
        InnerLangLesson {
            parent,
            translated_tx,
            voice_lessons: RwLock::new(BTreeMap::new()),
            drop_handler: Some(drop_handler),
        }
        .into()
    }

    pub(crate) async fn get_or_init(&mut self, voice: VoiceId) -> VoiceLesson {
        {
            let map = self.voice_lessons.read().await;
            if let Some(voice_lesson) = map.get(&voice).and_then(|weak| weak.upgrade()) {
                return voice_lesson.into();
            }
        }

        {
            let mut map = self.voice_lessons.write().await;
            if let Some(voice_lesson) = map.get(&voice).and_then(|weak| weak.upgrade()) {
                voice_lesson.into()
            } else {
                let voice_lesson = Arc::new(InnerVoiceLesson::new(self.clone(), voice.clone()));
                map.insert(voice, Arc::downgrade(&voice_lesson));
                voice_lesson.into()
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct VoiceLesson {
    inner: Arc<InnerVoiceLesson>,
}

impl VoiceLesson {
    pub(crate) fn voice_channel(&self) -> tokio::sync::broadcast::Receiver<Vec<u8>> {
        self.voice_lesson.subscribe()
    }

    pub(crate) fn lip_sync_channel(&self) -> tokio::sync::broadcast::Receiver<Vec<Viseme>> {
        self.lip_sync_tx.subscribe()
    }
}

impl Deref for VoiceLesson {
    type Target = InnerVoiceLesson;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl From<InnerVoiceLesson> for VoiceLesson {
    fn from(inner: InnerVoiceLesson) -> Self {
        VoiceLesson {
            inner: Arc::new(inner),
        }
    }
}

impl From<Arc<InnerVoiceLesson>> for VoiceLesson {
    fn from(inner: Arc<InnerVoiceLesson>) -> Self {
        VoiceLesson { inner }
    }
}

pub(crate) struct InnerVoiceLesson {
    lip_sync_tx: tokio::sync::broadcast::Sender<Vec<Viseme>>,
    voice_lesson: tokio::sync::broadcast::Sender<Vec<u8>>,
    drop_handler: Option<tokio::sync::oneshot::Sender<Signal>>,
}

#[derive(Debug)]
enum Signal {
    Stop,
}

impl InnerVoiceLesson {
    fn new(parent: LangLesson, voice: VoiceId) -> InnerVoiceLesson {
        let shared_voice_id: VoiceId = voice.clone();
        let (tx, rx) = tokio::sync::oneshot::channel::<Signal>();
        let mut translate_rx = parent.translated_tx.subscribe();
        let (voice_lesson, _) = tokio::sync::broadcast::channel::<Vec<u8>>(128);
        let shared_voice_lesson = voice_lesson.clone();
        let (lip_sync_tx, _) = tokio::sync::broadcast::channel::<Vec<Viseme>>(128);
        let shared_lip_sync_tx = lip_sync_tx.clone();
        let client = parent.parent.parent.polly_client.clone();
        // let lang: LanguageCode = parent.lang.clone().parse().expect("Invalid language code");
        tokio::spawn(async move {
            let fut = async {
                while let Ok(translated) = translate_rx.recv().await {
                    let res = synthesize_speech(&client, translated, shared_voice_id.clone()).await;
                    match res {
                        Ok((vec, audio)) => {
                            let _ = shared_lip_sync_tx.send(vec);
                            let _ = &shared_voice_lesson.send(audio);
                        }
                        Err(e) => {
                            return Err(e);
                        }
                    }
                }
                Ok(())
            };
            select! {
                res = fut => match res {
                    Ok(_) => {}
                    Err(e) => {
                        warn!("Error: {:?}", e);
                    }
                },
                _ = rx => {}
            }
        });

        InnerVoiceLesson {
            lip_sync_tx,
            voice_lesson,
            drop_handler: Some(tx),
        }
    }
}

impl Drop for InnerVoiceLesson {
    fn drop(&mut self) {
        if let Some(tx) = self.drop_handler.take() {
            let _ = tx.send(Signal::Stop);
        }
    }
}



#[derive(Debug)]
enum SynthesizeError {
    Polly(aws_sdk_polly::Error),
    Transmitting(aws_sdk_polly::error::BoxError),
}

async fn synthesize_speech(
    client: &aws_sdk_polly::Client,
    text: String,
    voice_id: VoiceId,
) -> Result<(Vec<Viseme>, Vec<u8>), SynthesizeError> {
    let audio_fut = client
        .synthesize_speech()
        .engine(Engine::Neural)
        .set_text(Some(text.clone()))
        .voice_id(voice_id.clone())
        .output_format(OutputFormat::Pcm)
        .send();
    let visemes_fut = client
        .synthesize_speech()
        .engine(Engine::Neural)
        .set_text(Some(text))
        .voice_id(voice_id)
        .speech_mark_types(SpeechMarkType::Viseme)
        .output_format(OutputFormat::Json)
        .send();
    let (audio_out, visemes_out) = try_join(audio_fut, visemes_fut)
        .await
        .map_err(|e| SynthesizeError::Polly(e.into()))?;
    let audio = audio_out
        .audio_stream
        .collect()
        .await
        .map_err(|e| SynthesizeError::Transmitting(e.into()))?
        .to_vec();
    let visemes = visemes_out
        .audio_stream
        .collect()
        .await
        .map_err(|e| SynthesizeError::Transmitting(e.into()))?
        .to_vec();
    let parsed: Vec<Viseme> = visemes
        .lines()
        .flat_map(|line| Ok::<Viseme, anyhow::Error>(serde_json::from_str::<Viseme>(&line?)?))
        .collect();
    Ok((parsed, audio))
}
