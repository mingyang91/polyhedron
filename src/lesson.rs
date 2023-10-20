use std::sync::{Arc, Weak};
use tokio::sync::RwLock;
use std::collections::BTreeMap;
use async_stream::stream;
use aws_config::SdkConfig;
use aws_sdk_polly::types::VoiceId;
use aws_sdk_transcribestreaming::primitives::Blob;
use aws_sdk_transcribestreaming::types::{AudioEvent, AudioStream, LanguageCode, MediaEncoding, TranscriptResultStream};
use futures_util::{StreamExt, TryStreamExt};

use tokio::select;
use crate::to_stream;

#[derive(Clone, Debug)]
pub struct LessonsManager {
    translate_client: aws_sdk_translate::Client,
    polly_client: aws_sdk_polly::Client,
    transcript_client: aws_sdk_transcribestreaming::Client,
    lessons: Arc<RwLock<BTreeMap<u32, Lesson>>>
}

impl LessonsManager {
    pub(crate) fn new(sdk_config: &SdkConfig) -> Self {
        let transcript_client = aws_sdk_transcribestreaming::Client::new(&sdk_config);
        let translate_client = aws_sdk_translate::Client::new(&sdk_config);
        let polly_client = aws_sdk_polly::Client::new(&sdk_config);
        LessonsManager {
            translate_client,
            polly_client,
            transcript_client,
            lessons: Arc::new(RwLock::new(BTreeMap::new()))
        }
    }

    pub(crate) async fn create_lesson(&self,
                                      id: u32,
                                      speaker_lang: LanguageCode) -> Lesson {
        let mut map = self.lessons.write().await;
        let lesson: Lesson = InnerLesson::new(self.clone(), id, speaker_lang).into();
        map.insert(id, lesson.clone());
        lesson
    }

    pub(crate) async fn get_lesson(&self, id: u32) -> Option<Lesson> {
        let map = self.lessons.read().await;
        map.get(&id).cloned()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Lesson {
    inner: Arc<InnerLesson>
}

impl Lesson {
    pub(crate) async fn get_or_init(&self, lang: String) -> LangLesson {
        {
            let map = self.inner.lang_lessons.read().await;
            if let Some(lang_lesson) = map.get(&lang).and_then(|weak| weak.upgrade()) {
                return lang_lesson.into();
            }
        }
        {
            let mut map = self.inner.lang_lessons.write().await;
            if let Some(lang_lesson) = map.get(&lang).and_then(|weak| weak.upgrade()) {
                lang_lesson.into()
            } else {
                let lang_lesson = LangLesson::new(
                    self.clone(),
                    lang.clone(),
                );
                map.insert(lang.clone(), Arc::downgrade(&lang_lesson.inner));
                lang_lesson
            }
        }
    }

    pub(crate) fn voice_channel(&self) -> tokio::sync::mpsc::Sender<Vec<u8>> {
        self.inner.speaker_voice_channel.clone()
    }

    pub(crate) fn transcript_channel(&self) -> tokio::sync::broadcast::Receiver<String> {
        self.inner.speaker_transcript.subscribe()
    }
}

impl From<InnerLesson> for Lesson {
    fn from(inner: InnerLesson) -> Self {
        Lesson {
            inner: Arc::new(inner)
        }
    }
}

#[derive(Debug)]
struct InnerLesson {
    parent: LessonsManager,
    id: u32,
    speaker_lang: LanguageCode,
    speaker_voice_channel: tokio::sync::mpsc::Sender<Vec<u8>>,
    speaker_transcript: tokio::sync::broadcast::Sender<String>,
    lang_lessons: RwLock<BTreeMap<String, Weak<InnerLangLesson>>>,
    drop_handler: Option<tokio::sync::oneshot::Sender<Signal>>,
}

impl InnerLesson {
    fn new(
        parent: LessonsManager,
        id: u32,
        speaker_lang: LanguageCode
    ) -> InnerLesson {
        let (speaker_transcript, _) = tokio::sync::broadcast::channel::<String>(128);
        let shared_speaker_transcript = speaker_transcript.clone();
        let (speaker_voice_channel, mut speaker_voice_rx) = tokio::sync::mpsc::channel(128);
        let (drop_handler, drop_rx) = tokio::sync::oneshot::channel::<Signal>();
        let transcript_client = parent.transcript_client.clone();
        let shared_speak_lang = speaker_lang.clone();

        tokio::spawn(async move {
            let fut = async {
                let input_stream = stream! {
                    while let Some(raw) = speaker_voice_rx.recv().await {
                        yield Ok(AudioStream::AudioEvent(AudioEvent::builder().audio_chunk(Blob::new(raw)).build()));
                    }
                };
                let output = transcript_client
                    .start_stream_transcription()
                    .language_code(shared_speak_lang)//LanguageCode::EnGb
                    .media_sample_rate_hertz(16000)
                    .media_encoding(MediaEncoding::Pcm)
                    .audio_stream(input_stream.into())
                    .send()
                    .await
                    .map_err(|e| crate::StreamTranscriptionError::EstablishStreamError(Box::new(e)))?;

                let mut output_stream = to_stream(output);
                output_stream
                    .try_for_each(|text| async {
                        let _ = shared_speaker_transcript.send(text);
                        Ok(())
                    })
                    .await?;
                Ok(()) as Result<(), crate::StreamTranscriptionError>
            };
            select! {
                res = fut => {
                    if let Err(e) = res {
                        println!("Error: {:?}", e);
                    }
                }
                _ = drop_rx => {}
            }
        });

        InnerLesson {
            parent,
            id,
            speaker_lang,
            speaker_voice_channel,
            speaker_transcript: speaker_transcript,
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


struct InnerLangLesson {
    parent: Lesson,
    lang: String,
    translated_tx: tokio::sync::broadcast::Sender<String>,
    voice_lessons: RwLock<BTreeMap<VoiceId, Weak<InnerVoiceLesson>>>,
    drop_handler: Option<tokio::sync::oneshot::Sender<Signal>>,
}

#[derive(Clone)]
pub(crate) struct LangLesson {
    inner: Arc<InnerLangLesson>
}

impl LangLesson {
    pub(crate) fn translated_channel(&self) -> tokio::sync::broadcast::Receiver<String> {
        self.inner.translated_tx.subscribe()
    }
}

impl From<InnerLangLesson> for LangLesson {
    fn from(inner: InnerLangLesson) -> Self {
        LangLesson {
            inner: Arc::new(inner)
        }
    }
}

impl From<Arc<InnerLangLesson>> for LangLesson {
    fn from(inner: Arc<InnerLangLesson>) -> Self {
        LangLesson {
            inner
        }
    }
}

impl LangLesson {
    fn new(
        parent: Lesson,
        lang: String,
    ) -> Self {
        let shared_lang = lang.clone();;
        let shared_speaker_lang = parent.inner.speaker_lang.clone();
        let (translated_tx, _) = tokio::sync::broadcast::channel::<String>(128);
        let shared_translated_tx = translated_tx.clone();
        let mut transcript_rx = parent.inner.speaker_transcript.subscribe();
        let translate_client = parent.inner.parent.translate_client.clone();
        let (drop_handler, drop_rx) = tokio::sync::oneshot::channel::<Signal>();
        tokio::spawn(async move {
            let fut = async {
                while let Ok(text) = transcript_rx.recv().await {
                    let output = translate_client
                        .translate_text()
                        .text(text)
                        .source_language_code(shared_speaker_lang.as_str())
                        .target_language_code(shared_lang.clone())
                        .send()
                        .await;
                    match output {
                        Ok(res) => {
                            if let Some(translated) = res.translated_text {
                                let _ = shared_translated_tx.send(translated);
                            }
                        },
                        Err(e) => {
                            return Err(e);
                        }
                        _ => {}
                    }
                }
                Ok(())
            };

            select! {
                res = fut => {
                    if let Err(e) = res {
                        println!("Error: {:?}", e);
                    }
                }
                _ = drop_rx => {}
            }
        });
        InnerLangLesson {
            parent,
            lang,
            translated_tx,
            voice_lessons: RwLock::new(BTreeMap::new()),
            drop_handler: Some(drop_handler),
        }.into()
    }

    pub(crate) async fn get_or_init(&mut self, voice: VoiceId) -> VoiceLesson {
        {
            let map = self.inner.voice_lessons.read().await;
            if let Some(voice_lesson) = map.get(&voice).and_then(|weak| weak.upgrade()) {
                return voice_lesson.into();
            }
        }

        {
            let mut map = self.inner.voice_lessons.write().await;
            if let Some(voice_lesson) = map.get(&voice).and_then(|weak| weak.upgrade()) {
                voice_lesson.into()
            } else {
                let voice_lesson = Arc::new(InnerVoiceLesson::new(
                    self.clone(),
                    voice.clone(),
                ));
                map.insert(voice, Arc::downgrade(&voice_lesson));
                voice_lesson.into()
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct VoiceLesson {
    inner: Arc<InnerVoiceLesson>
}

impl VoiceLesson {
    pub(crate) fn voice_channel(&self) -> tokio::sync::broadcast::Receiver<Vec<u8>> {
        self.inner.voice_lesson.subscribe()
    }
}

impl From<InnerVoiceLesson> for VoiceLesson {
    fn from(inner: InnerVoiceLesson) -> Self {
        VoiceLesson {
            inner: Arc::new(inner)
        }
    }
}

impl From<Arc<InnerVoiceLesson>> for VoiceLesson {
    fn from(inner: Arc<InnerVoiceLesson>) -> Self {
        VoiceLesson {
            inner
        }
    }
}

struct InnerVoiceLesson {
    parent: LangLesson,
    voice: VoiceId,
    voice_lesson: tokio::sync::broadcast::Sender<Vec<u8>>,
    drop_handler: Option<tokio::sync::oneshot::Sender<Signal>>,
}

#[derive(Debug)]
enum Signal {
    Stop,
}

impl InnerVoiceLesson {
    fn new(
        parent: LangLesson,
        voice: VoiceId,
    ) -> InnerVoiceLesson {
        let shared_voice_id: VoiceId = voice.clone();
        let (tx, rx) = tokio::sync::oneshot::channel::<Signal>();
        let mut translate_rx = parent.inner.translated_tx.subscribe();
        let (voice_lesson, _) = tokio::sync::broadcast::channel::<Vec<u8>>(128);
        let shared_voice_lesson = voice_lesson.clone();
        let client = parent.inner.parent.inner.parent.polly_client.clone();
        // let lang: LanguageCode = parent.inner.lang.clone().parse().expect("Invalid language code");
        tokio::spawn(async move {
            let fut = async {
                while let Ok(translated) = translate_rx.recv().await {
                    let res = client.synthesize_speech()
                        .set_text(Some(translated))
                        .voice_id(shared_voice_id.clone())
                        .output_format("pcm".into())
                        // .language_code(lang)
                        // .language_code("cmn-CN".into())
                        .send()
                        .await;
                    match res {
                        Ok(mut synthesized) => {
                            while let Some(Ok(bytes)) = synthesized.audio_stream.next().await {
                                let _ = &shared_voice_lesson.send(bytes.to_vec());
                            }
                        },
                        Err(e) => {
                            return Err(e);
                        }
                    }
                }
                Ok(())
            };
            select! {
                _ = fut => {}
                _ = rx => {}
            }
        });

        InnerVoiceLesson {
            parent,
            voice,
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

