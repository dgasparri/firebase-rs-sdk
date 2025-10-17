use rand::Rng;
use std::sync::{LazyLock, Mutex};

const PUSH_CHARS: &[u8; 64] = b"-0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ_abcdefghijklmnopqrstuvwxyz";

struct PushState {
    last_push_time: u64,
    last_rand_chars: [u8; 12],
}

impl PushState {
    fn new() -> Self {
        Self {
            last_push_time: 0,
            last_rand_chars: [0; 12],
        }
    }
}

static PUSH_STATE: LazyLock<Mutex<PushState>> = LazyLock::new(|| Mutex::new(PushState::new()));

/// Port of `nextPushId` from `packages/database/src/core/util/NextPushId.ts`.
pub(crate) fn next_push_id(mut now: u64) -> String {
    let mut state = PUSH_STATE.lock().unwrap();
    let duplicate_time = now == state.last_push_time;
    state.last_push_time = now;

    let mut timestamp_chars = [0u8; 8];
    for slot in timestamp_chars.iter_mut().rev() {
        let index = (now % 64) as usize;
        now /= 64;
        *slot = PUSH_CHARS[index];
    }
    debug_assert!(now == 0, "push id timestamp overflowed base64 encoding");

    if !duplicate_time {
        let mut rng = rand::thread_rng();
        for char_slot in state.last_rand_chars.iter_mut() {
            *char_slot = rng.gen_range(0..64);
        }
    } else {
        let mut index = state.last_rand_chars.len();
        while index > 0 && state.last_rand_chars[index - 1] == 63 {
            state.last_rand_chars[index - 1] = 0;
            index -= 1;
        }
        if index == 0 {
            // Extremely unlikely overflow; fall back to the lowest value so a
            // subsequent millisecond tick reseeds the sequence.
            state.last_rand_chars[0] = 0;
        } else {
            state.last_rand_chars[index - 1] += 1;
        }
    }

    let mut id = String::with_capacity(20);
    for ch in &timestamp_chars {
        id.push(*ch as char);
    }
    for &rand_index in &state.last_rand_chars {
        id.push(PUSH_CHARS[rand_index as usize] as char);
    }

    debug_assert_eq!(id.len(), 20, "push id should be 20 characters");

    id
}
