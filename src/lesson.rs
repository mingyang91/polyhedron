use serde::Deserialize;
use std::sync::{Arc, Weak};
use tokio::sync::RwLock;
use std::collections::BTreeMap;
use aws_config::SdkConfig;
use tokio::select;

#[derive(Clone)]
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
                                      speaker_lang: String) -> Lesson {
        let mut map = self.lessons.write().await;
        let lesson: Lesson = InnerLesson::new(id, speaker_lang).into();
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
        let map = self.inner.lang_lessons.read().await;
        if let Some(lang_lesson) = map.get(&lang).and_then(|weak| weak.upgrade()) {
            return lang_lesson.into();
        }
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

impl From<InnerLesson> for Lesson {
    fn from(inner: InnerLesson) -> Self {
        Lesson {
            inner: Arc::new(inner)
        }
    }
}

#[derive(Debug)]
struct InnerLesson {
    id: u32,
    speaker_lang: String,
    speaker_voice_channel: tokio::sync::mpsc::Sender<Vec<u8>>,
    speaker_transcript: tokio::sync::broadcast::Sender<String>,
    lang_lessons: RwLock<BTreeMap<String, Weak<InnerLangLesson>>>,
    drop_handler: Option<tokio::sync::oneshot::Sender<Signal>>,
}

impl InnerLesson {
    fn new(
        id: u32,
        speaker_lang: String
    ) -> InnerLesson {
        let (speaker_transcript, _) = tokio::sync::broadcast::channel::<String>(128);
        let (speaker_voice_channel, mut speaker_voice_rx) = tokio::sync::mpsc::channel(128);
        let (drop_handler, mut drop_rx) = tokio::sync::oneshot::channel::<Signal>();

        tokio::spawn(async move {
            let fut = async {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            };
            select! {
                _ = fut => {}
                _ = drop_rx => {}
            }
        });

        InnerLesson {
            id,
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
    lang: String,
    translated: tokio::sync::broadcast::Sender<String>,
    voice_lessons: RwLock<BTreeMap<String, Weak<InnerVoiceLesson>>>
}

#[derive(Clone)]
pub(crate) struct LangLesson {
    inner: Arc<InnerLangLesson>
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
        let (translated, _) = tokio::sync::broadcast::channel::<String>(128);
        InnerLangLesson {
            parent,
            lang,
            translated,
            voice_lessons: RwLock::new(BTreeMap::new()),
        }.into()
    }

    async fn get_or_init(&mut self, voice: String) -> VoiceLesson {
        let map = self.inner.voice_lessons.read().await;
        if let Some(voice_lesson) = map.get(&voice).and_then(|weak| weak.upgrade()) {
            return voice_lesson.into();
        }
        let mut map = self.inner.voice_lessons.write().await;
        if let Some(voice_lesson) = map.get(&voice).and_then(|weak| weak.upgrade()) {
            voice_lesson.into()
        } else {
            let voice_lesson = Arc::new(InnerVoiceLesson::new(
                self.clone(),
                voice.clone(),
            ));
            map.insert(voice.clone(), Arc::downgrade(&voice_lesson));
            voice_lesson.into()
        }
    }
}

#[derive(Clone)]
struct VoiceLesson {
    inner: Arc<InnerVoiceLesson>
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
    voice: String,
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
        voice: String,
    ) -> InnerVoiceLesson {
        let (tx, rx) = tokio::sync::oneshot::channel::<Signal>();
        let (voice_lesson, _) = tokio::sync::broadcast::channel::<Vec<u8>>(128);
        tokio::spawn(async move {
            let fut = async {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
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

