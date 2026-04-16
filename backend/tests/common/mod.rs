use std::sync::Once;

static LOAD_DOTENV: Once = Once::new();

pub fn test_env(key: &str) -> Option<String> {
    LOAD_DOTENV.call_once(|| {
        let _ = dotenvy::from_filename(".env.test.local");
    });

    std::env::var(key).ok().filter(|value| !value.trim().is_empty())
}

pub fn required_test_env(key: &str) -> String {
    match test_env(key) {
        Some(value) => value,
        None => {
            eprintln!("Ignorando teste real: variável {key} não definida em backend/.env.test.local");
            String::new()
        }
    }
}

pub fn skip_if_missing(value: &str) -> bool {
    value.trim().is_empty()
}
