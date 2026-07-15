use audiobookgen_core::cache::segment_cache_key;
use audiobookgen_core::model::{
    Fragment, FragmentKind, FragmentLocator, NarrationProfile,
};
use chrono::Utc;
use uuid::Uuid;

fn fragment() -> Fragment {
    let book_id = Uuid::new_v4();
    Fragment {
        id: Uuid::new_v4(),
        book_id,
        chapter_id: Uuid::new_v4(),
        chapter_index: 0,
        index: 0,
        source_text: "The name is Siobhan.".into(),
        spoken_text: "The name is Shivawn.".into(),
        kind: FragmentKind::Sentence,
        locator: FragmentLocator {
            href: "chapter.xhtml".into(),
            css_selector: None,
            text_occurrence: 0,
            source_text_hash: "fixture".into(),
            cfi: None,
        },
        pause_after_ms: 340,
        cache_key: String::new(),
    }
}

fn profile(book_id: Uuid, voice: &str, speed: f32) -> NarrationProfile {
    NarrationProfile {
        id: Uuid::new_v4(),
        book_id,
        name: voice.into(),
        voice: voice.into(),
        speed,
        model_revision: "hexgrad/Kokoro-82M".into(),
        model_sha256: Some("model-sha".into()),
        normalization_version: "en-v1".into(),
        planner_version: "sentence-v1".into(),
        created_at: Utc::now(),
    }
}

#[test]
fn cache_key_changes_with_voice_and_speed() {
    let fragment = fragment();
    let heart = profile(fragment.book_id, "af_heart", 1.0);
    let bella = profile(fragment.book_id, "af_bella", 1.0);
    let faster = profile(fragment.book_id, "af_heart", 1.1);

    assert_ne!(segment_cache_key(&fragment, &heart), segment_cache_key(&fragment, &bella));
    assert_ne!(segment_cache_key(&fragment, &heart), segment_cache_key(&fragment, &faster));
}

#[test]
fn cache_key_changes_with_spoken_pronunciation() {
    let mut fragment = fragment();
    let profile = profile(fragment.book_id, "af_heart", 1.0);
    let original = segment_cache_key(&fragment, &profile);
    fragment.spoken_text = "The name is See oh ban.".into();
    assert_ne!(original, segment_cache_key(&fragment, &profile));
}
