use rand;
use rand::RngExt;

const MODIFIERS: [&str; 2] = ["Scornful", "Arrogant"];

const NOUNS: [&str; 2] = ["Abbot", "Soldier"];

pub fn get_agent_name() -> String {
    let mut rng = rand::rng();

    let modifier = MODIFIERS[rng.random_range(0..MODIFIERS.len())];
    let noun = NOUNS[rng.random_range(0..NOUNS.len())];

    format!("{modifier} {noun}")
}
