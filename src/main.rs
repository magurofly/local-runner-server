use std::{collections::HashMap, fs::File, io::Read, path::Path, process::{Command, ExitStatus, Stdio}, sync::{Mutex, OnceLock}, time::Duration};

use actix_cors::Cors;
use actix_web::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[allow(non_snake_case)]
#[derive(Deserialize)]
struct Req {
    mode: String,
    compilerName: Option<String>,
    sourceCode: Option<String>,
    stdin: Option<String>,
}

#[allow(non_snake_case)]
#[derive(Serialize)]
struct CompilerInfo<'a> {
    language: &'a str,
    compilerName: &'a str,
    label: &'a str,
}

#[allow(non_snake_case)]
#[derive(Serialize)]
struct RunResult {
    status: &'static str,
    stdout: Option<String>,
    stderr: Option<String>,
    exit_code: Option<i32>,
    time: Option<f64>,
    memory: Option<f64>,
}

#[derive(Deserialize)]
struct Config {
    bind: String,
    compilers: HashMap<String, Compiler>
}

#[derive(Deserialize)]
struct Compiler {
    language: String,
    label: String,
    copy_files: Vec<(String, String)>,
    source_filename: String,
    compile_command: Vec<String>,
    run_command: Vec<String>,
}

static CONFIG: OnceLock<Config> = OnceLock::new();

fn json_response(data: impl Serialize) -> HttpResponse {
    HttpResponse::Ok()
        .append_header(("Content-Type", "application/json"))
        .body(serde_json::to_string(&data).unwrap())
}

fn error_body(err: &str, msg: &str) -> HttpResponse {
    let mut map = HashMap::new();
    map.insert("status", err);
    map.insert("stderr", msg);
    json_response(map)
}

static COMPILE_MUTEX: Mutex<()> = Mutex::new(());

#[post("/")]
async fn api(req: web::Json<Req>) -> impl Responder {
    match req.mode.as_str() {
        "list" => {
            let data = CONFIG.get().unwrap().compilers.iter().map(|(name, compiler)| CompilerInfo {
                language: &compiler.language,
                compilerName: name,
                label: &compiler.label,
            }).collect::<Vec<_>>();
            json_response(data)
        }
        "run" => {
            let Some(compiler_name) = &req.compilerName else { return error_body("internalError", "compilerName not given") };
            let Some(source_code) = &req.sourceCode else { return error_body("internalError", "sourceCode not given") };
            let stdin = req.stdin.as_ref().map(String::as_str).unwrap_or("");

            let program_id = {
                let lock = COMPILE_MUTEX.lock().unwrap();
                let program_id = match compile(compiler_name, source_code).await {
                    Ok(program_id) => program_id,
                    Err(msg) => return error_body("compileError", &format!("{:?}\n{:?}", msg, msg.source())),
                };
                drop(lock);
                program_id
            };

            let result = match run(compiler_name, &program_id, stdin).await {
                Ok(result) => result,
                Err(msg) => return error_body("internalError", &format!("{:?}\n{:?}", msg, msg.source())),
            };

            json_response(&result)
        }
        _ => {
            HttpResponse::Ok().body(r#"Error: invalid mode"#)
        }
    }
}

async fn compile(compiler_name: &str, source_code: &str) -> Result<String, Box<dyn std::error::Error>> {
    let Some(compiler) = CONFIG.get().unwrap().compilers.get(compiler_name) else { return Err("undefined compilerName".into()) };

    let program_id = format!("{compiler_name}-{}", hex::encode(Sha256::digest(source_code)));
    let dir = Path::new("program").join(&program_id);

    if std::fs::exists(&dir).unwrap() {
        // コンパイルが完了するまで待つ
        actix_web::rt::time::sleep(Duration::from_millis(100)).await;
        while !std::fs::exists(dir.join("compile_status.txt"))? {
            actix_web::rt::time::sleep(Duration::from_millis(100)).await;
        }

        let mut status = String::new();
        File::open(dir.join("compile_status.txt"))?.read_to_string(&mut status)?;
        let status = status.parse::<i32>()?;
        if status != 0 {
            let mut compile_error = String::new();
            File::open(dir.join("compile_error.txt"))?.read_to_string(&mut compile_error)?;
            return Err(compile_error.into());   
        }

        return Ok(program_id);
    }

    std::fs::create_dir(&dir)?;

    for (src, dst) in &compiler.copy_files {
        if let Some(parent) = Path::new(dst).parent() {
            std::fs::create_dir_all(dir.join(&parent))?;
        }

        let src_path = Path::new("template").join(src);
        let dst_path = dir.join(dst);

        std::fs::copy(src_path, dst_path)?;
    }

    if let Some(parent) = Path::new(&compiler.source_filename).parent() {
        std::fs::create_dir_all(dir.join(&parent))?;
    }
    let source_path = dir.join(&compiler.source_filename);
    std::fs::write(source_path, source_code)?;

    let mut compile_command = Command::new(&compiler.compile_command[0]);
    compile_command.args(&compiler.compile_command[1 ..]);
    compile_command.stderr(Stdio::from(File::create(dir.join("compile_error.txt"))?));
    compile_command.current_dir(&dir);

    let status = compile_command.status()?.code().unwrap();
    std::fs::write(dir.join("compile_status.txt"), status.to_string())?;

    if status != 0 {
        let mut compile_error = String::new();
        File::open(dir.join("compile_error.txt"))?.read_to_string(&mut compile_error)?;
        return Err(compile_error.into());
    }

    Ok(program_id)
}

async fn run(compiler_name: &str, program_id: &str, stdin: &str) -> Result<RunResult, Box<dyn std::error::Error>> {
    let dir = Path::new("program").join(&program_id);
    
    let compiler = CONFIG.get().unwrap().compilers.get(compiler_name).unwrap();

    let prefix = format!("{}", Uuid::new_v4());
    let stdin_filename = format!("{prefix}-stdin.txt");
    let stdout_filename = format!("{prefix}-stdout.txt");
    let stderr_filename = format!("{prefix}-stderr.txt");
    let time_filename = format!("{prefix}-time.txt");
    
    std::fs::write(dir.join(&stdin_filename), stdin)?;

    let mut run_command = Command::new("time");
    run_command.args(["-q", "-f", "%e %M", "-o", &time_filename]);
    run_command.args(&compiler.run_command);
    run_command.stdin(Stdio::from(File::open(dir.join(&stdin_filename))?));
    run_command.stdout(Stdio::from(File::create(dir.join(&stdout_filename))?));
    run_command.stderr(Stdio::from(File::create(dir.join(&stderr_filename))?));
    run_command.current_dir(&dir);

    let status = run_command.status()?;

    let (time, memory) = {
        let mut time_memory = String::new();
        File::open(dir.join(&time_filename))?.read_to_string(&mut time_memory)?;
        let mut it = time_memory.split_ascii_whitespace();
        let time = it.next().unwrap().parse::<f64>()? * 1E+3;
        let memory = it.next().unwrap().parse::<f64>()?;
        (time, memory)
    };

    let mut stdout = String::new();
    File::open(dir.join(&stdout_filename))?.read_to_string(&mut stdout)?;

    let mut stderr = String::new();
    File::open(dir.join(&stderr_filename))?.read_to_string(&mut stderr)?;

    let exit_code = status.code().unwrap();

    std::fs::remove_file(dir.join(&stdin_filename))?;
    std::fs::remove_file(dir.join(&stdout_filename))?;
    std::fs::remove_file(dir.join(&stderr_filename))?;
    std::fs::remove_file(dir.join(&time_filename))?;

    Ok(RunResult {
        status: "success",
        stdout: Some(stdout),
        stderr: Some(stderr),
        exit_code: Some(exit_code),
        memory: Some(memory),
        time: Some(time),
    })
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let config = toml::from_str::<Config>(&std::fs::read_to_string("config.toml")?).unwrap();
    CONFIG.get_or_init(|| config );

    HttpServer::new(|| {
        App::new()
            .wrap(
                Cors::default()
                    .allow_any_origin()
                    .allow_any_method()
                    .allow_any_header()
            )
            .service(api)
    })
        .bind(&CONFIG.get().unwrap().bind)?
        .run()
        .await
}