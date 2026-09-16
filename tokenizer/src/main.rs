use std::process::ExitCode;

fn main() -> ExitCode {
    let mut arguments = std::env::args();
    let executable = arguments.next().unwrap_or_else(|| "tokenizer".to_owned());
    let Some(model_id) = arguments.next() else {
        eprintln!("usage: {executable} <model-id> <text>");
        return ExitCode::from(2);
    };
    let text = arguments.collect::<Vec<_>>().join(" ");

    match tokenizer::tokenize(&model_id, &text) {
        Ok(count) => {
            println!("{count}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("tokenizer: {error}");
            ExitCode::FAILURE
        }
    }
}
