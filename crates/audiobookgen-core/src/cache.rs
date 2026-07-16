use crate::model::{Fragment, NarrationProfile};
use sha2::{Digest, Sha256};

pub fn segment_cache_key(fragment: &Fragment, profile: &NarrationProfile) -> String {
    let mut digest = Sha256::new();
    update(&mut digest, fragment.spoken_text.as_bytes());
    update(&mut digest, &fragment.pause_after_ms.to_le_bytes());
    update(&mut digest, profile.voice.as_bytes());
    update(&mut digest, &profile.speed.to_bits().to_le_bytes());
    update(&mut digest, profile.model_revision.as_bytes());
    update(
        &mut digest,
        profile
            .model_sha256
            .as_deref()
            .unwrap_or("unverified")
            .as_bytes(),
    );
    update(&mut digest, profile.normalization_version.as_bytes());
    update(&mut digest, profile.planner_version.as_bytes());
    hex::encode(digest.finalize())
}

fn update(digest: &mut Sha256, value: &[u8]) {
    digest.update((value.len() as u64).to_le_bytes());
    digest.update(value);
}
