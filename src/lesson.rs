use std::sync::{Arc, Weak};
use tokio::sync::RwLock;
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::io::BufRead;
use async_stream::stream;
use aws_config::SdkConfig;
use aws_sdk_polly::primitives::ByteStream;
use aws_sdk_polly::types::{Engine, OutputFormat, SpeechMarkType, VoiceId};
use aws_sdk_transcribestreaming::operation::start_stream_transcription::StartStreamTranscriptionOutput;
use aws_sdk_transcribestreaming::primitives::Blob;
use aws_sdk_transcribestreaming::types::{AudioEvent, AudioStream, LanguageCode, MediaEncoding, TranscriptResultStream};
use futures_util::{Stream, StreamExt, TryStreamExt};
use futures_util::future::try_join;
use serde::{Deserialize, Serialize};

use tokio::select;

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
        let lesson: Lesson = InnerLesson::new(self.clone(), speaker_lang).into();
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
    speaker_lang: LanguageCode,
    speaker_voice_channel: tokio::sync::mpsc::Sender<Vec<u8>>,
    speaker_transcript: tokio::sync::broadcast::Sender<String>,
    lang_lessons: RwLock<BTreeMap<String, Weak<InnerLangLesson>>>,
    drop_handler: Option<tokio::sync::oneshot::Sender<Signal>>,
}

impl InnerLesson {
    fn new(
        parent: LessonsManager,
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
                    .map_err(|e| StreamTranscriptionError::EstablishStreamError(Box::new(e)))?;

                let output_stream = to_stream(output);
                output_stream
                    .try_for_each(|text| async {
                        let _ = shared_speaker_transcript.send(text);
                        Ok(())
                    })
                    .await?;
                Ok(()) as Result<(), StreamTranscriptionError>
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


struct InnerLangLesson {
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
        let shared_lang = lang.clone();
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

    pub(crate) fn lip_sync_channel(&self) -> tokio::sync::broadcast::Receiver<Vec<Viseme>> {
        self.inner.lip_sync_tx.subscribe()
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
    lip_sync_tx: tokio::sync::broadcast::Sender<Vec<Viseme>>,
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
        let (lip_sync_tx, _) = tokio::sync::broadcast::channel::<Vec<Viseme>>(128);
        let shared_lip_sync_tx = lip_sync_tx.clone();
        let client = parent.inner.parent.inner.parent.polly_client.clone();
        // let lang: LanguageCode = parent.inner.lang.clone().parse().expect("Invalid language code");
        tokio::spawn(async move {
            let fut = async {
                while let Ok(translated) = translate_rx.recv().await {
                    let res = synthesize_speech(&client, translated, shared_voice_id.clone()).await;
                    match res {
                        Ok((vec, mut audio_stream)) => {
                            let _ = shared_lip_sync_tx.send(vec);
                            while let Some(Ok(bytes)) = audio_stream.next().await {
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
                res = fut => match res {
                    Ok(_) => {}
                    Err(e) => {
                        println!("Error: {:?}", e);
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


fn to_stream(mut output: StartStreamTranscriptionOutput) -> impl Stream<Item=Result<String, StreamTranscriptionError>> {
    stream! {
        while let Some(event) = output
            .transcript_result_stream
            .recv()
            .await
            .map_err(|e| StreamTranscriptionError::TranscriptResultStreamError(Box::new(e)))? {
            match event {
                TranscriptResultStream::TranscriptEvent(transcript_event) => {
                    let transcript = transcript_event.transcript.expect("transcript");
                    for result in transcript.results.unwrap_or_default() {
                        if !result.is_partial {
                            let first_alternative = &result.alternatives.as_ref().expect("should have")[0];
                            let slice = first_alternative.transcript.as_ref().expect("should have");
                            yield Ok(slice.clone());
                        }
                    }
                }
                _ => yield Err(StreamTranscriptionError::Unknown),
            }
        }
    }
}

// {"time":180,"type":"viseme","value":"r"}
#[derive(Debug, Deserialize, Clone, Serialize)]
pub(crate) struct Viseme {
    time: u32,
    value: String,
}

#[derive(Debug)]
enum SynthesizeError {
    Polly(aws_sdk_polly::Error),
    Transmitting(aws_sdk_polly::error::BoxError),
}

async fn synthesize_speech(client: &aws_sdk_polly::Client,
                           text: String,
                           voice_id: VoiceId) -> Result<(Vec<Viseme>, ByteStream), SynthesizeError> {
    let audio_fut = client.synthesize_speech()
        .engine(Engine::Neural)
        .set_text(Some(text.clone()))
        .voice_id(voice_id.clone())
        .output_format(OutputFormat::Pcm)
        .send();
    let visemes_fut = client.synthesize_speech()
        .engine(Engine::Neural)
        .set_text(Some(text))
        .voice_id(voice_id)
        .speech_mark_types(SpeechMarkType::Viseme)
        .output_format(OutputFormat::Json)
        .send();
    let (audio, visemes) = try_join(audio_fut, visemes_fut)
        .await
        .map_err(|e| SynthesizeError::Polly(e.into()))?;
    let visemes = visemes.audio_stream.collect().await
        .map_err(|e| SynthesizeError::Transmitting(e.into()))?.to_vec();
    let parsed: Vec<Viseme> = visemes
        .lines()
        .filter_map(|line| line.ok())
        .filter_map(|line| serde_json::from_str::<Viseme>(&line).ok())
        .collect();
    Ok((parsed, audio.audio_stream))
}


#[derive(Debug)]
enum StreamTranscriptionError {
    EstablishStreamError(Box<dyn Error + Send + Sync>),
    TranscriptResultStreamError(Box<dyn Error + Send + Sync>),
    Unknown
}


impl Display for StreamTranscriptionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            StreamTranscriptionError::EstablishStreamError(e) => write!(f, "EstablishStreamError: {}", e),
            StreamTranscriptionError::TranscriptResultStreamError(e) => write!(f, "TranscriptResultStreamError: {}", e),
            StreamTranscriptionError::Unknown => write!(f, "Unknown"),
        }
    }
}

impl Error for StreamTranscriptionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            StreamTranscriptionError::EstablishStreamError(e) => Some(e.as_ref()),
            StreamTranscriptionError::TranscriptResultStreamError(e) => Some(e.as_ref()),
            StreamTranscriptionError::Unknown => None,
        }
    }
}

