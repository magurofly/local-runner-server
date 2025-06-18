use std::{collections::HashMap, env::{current_dir, set_current_dir}, fs::File, io::Read, path::Path, process::{Command, ExitStatus, Stdio}, sync::OnceLock};

use actix_cors::Cors;
use actix_web::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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

            let program_id = match compile(compiler_name, source_code).await {
                Ok(program_id) => program_id,
                Err(msg) => return error_body("compileError", &format!("{:?}", msg)),
            };

            let result = match run(compiler_name, &program_id, stdin).await {
                Ok(result) => result,
                Err(msg) => return error_body("internalError", &format!("{:?}", msg)),
            };

            json_response(&result)
        }
        _ => {
            HttpResponse::Ok().body(r#"Error: invalid mode"#)
        }
    }
}

fn cd_temporary<T>(path: impl AsRef<Path>, f: impl FnOnce() -> T) -> T {
    let orig_dir = current_dir().unwrap();
    set_current_dir(path).unwrap();
    let x = f();
    set_current_dir(orig_dir).unwrap();
    x
}

async fn compile(compiler_name: &str, source_code: &str) -> Result<String, Box<dyn std::error::Error>> {
    let Some(compiler) = CONFIG.get().unwrap().compilers.get(compiler_name) else { return Err("undefined compilerName".into()) };

    let program_id = hex::encode(Sha256::digest(source_code));
    let dir = Path::new("program").join(&program_id);

    if std::fs::exists(&dir)? {
        //TODO: when collision occurs
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

    cd_temporary(&dir, || {
        let mut compile_command = Command::new(&compiler.compile_command[0]);
        compile_command.args(&compiler.compile_command[1 ..]);
        compile_command.stderr(Stdio::from(File::create_new("compile_error.txt")?));

        let status = compile_command.status()?;
        if status.code() != Some(0) {
            let mut compile_error = String::new();
            File::open("compile_error.txt")?.read_to_string(&mut compile_error)?;
            return Err(compile_error.into());
        }

        Ok(program_id)
    })
}

async fn run(compiler_name: &str, program_id: &str, stdin: &str) -> Result<RunResult, Box<dyn std::error::Error>> {
    let compiler = CONFIG.get().unwrap().compilers.get(compiler_name).unwrap();

    let dir = Path::new("program").join(&program_id);
    cd_temporary(&dir, || {
        std::fs::write("stdin.txt", stdin)?;

        let mut run_command = Command::new("time");
        run_command.args(["-q", "-f", "%e %M", "-o", "time.txt"]);
        run_command.args(&compiler.run_command);
        run_command.stdin(Stdio::from(File::open("stdin.txt")?));
        run_command.stdout(Stdio::from(File::create("stdout.txt")?));
        run_command.stderr(Stdio::from(File::create("stderr.txt")?));

        let status = run_command.status()?;

        let (time, memory) = {
            let mut time_memory = String::new();
            File::open("time.txt")?.read_to_string(&mut time_memory)?;
            let mut it = time_memory.split_ascii_whitespace();
            let time = it.next().unwrap().parse::<f64>()? * 1E+3;
            let memory = it.next().unwrap().parse::<f64>()?;
            (time, memory)
        };

        let mut stdout = String::new();
        File::open("stdout.txt")?.read_to_string(&mut stdout)?;

        let mut stderr = String::new();
        File::open("stderr.txt")?.read_to_string(&mut stderr)?;

        let exit_code = status.code().unwrap();

        Ok(RunResult {
            status: "success",
            stdout: Some(stdout),
            stderr: Some(stderr),
            exit_code: Some(exit_code),
            memory: Some(memory),
            time: Some(time),
        })
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