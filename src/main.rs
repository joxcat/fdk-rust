#![forbid(unsafe_code)]
#![deny(
    clippy::complexity,
    clippy::perf,
    clippy::checked_conversions,
    clippy::filter_map_next
)]
#![warn(
    clippy::style,
    clippy::map_unwrap_or,
    clippy::missing_const_for_fn,
    clippy::use_self,
    future_incompatible,
    rust_2018_idioms,
    nonstandard_style
)]
// with configurable values
#![warn(
    clippy::blacklisted_name,
    clippy::cognitive_complexity,
    clippy::disallowed_method,
    clippy::fn_params_excessive_bools,
    clippy::struct_excessive_bools,
    clippy::too_many_lines,
    clippy::type_complexity,
    clippy::trivially_copy_pass_by_ref,
    clippy::type_repetition_in_bounds,
    clippy::unreadable_literal
)]
#![deny(clippy::wildcard_imports)]
// crate-specific exceptions:
#![allow(dead_code)]
#![cfg(unix)]

use thiserror::Error;

#[derive(Error, Debug)]
enum RustFdkError {
    #[error("Error thrown by the FDK")]
    Fdk(FdkError),
    #[error("...")]
    Io(#[from] std::io::Error),
}
impl<T> From<FdkError> for RustFdkResult<T> {
    fn from(err: FdkError) -> Self {
        Self::Err(RustFdkError::Fdk(err))
    }
}

type RustFdkResult<T> = Result<T, RustFdkError>;

#[derive(Debug, Clone)]
struct FdkEnv {
    fn_listener: Option<String>,
    fn_format: Option<String>,
    fn_logframe_name: Option<String>,
    fn_logframe_hdr: Option<String>,
    fdk_log_threshold: Option<String>,
    fn_app_id: Option<String>,
    fn_fn_id: Option<String>,
    fn_memory: Option<String>,
}

#[derive(Debug, Clone)]
struct FdkError {
    pub message: String,
    pub backtrace: Vec<String>,
}
impl FdkError {
    fn new(message: &str) -> Self {
        Self {
            message: message.to_string(),
            backtrace: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct FdkRunner {
    env_context: Rc<FdkEnv>,
}
impl FdkRunner {
    const FDK_LOG_DEBUG: u32 = 0;
    const FDK_LOG_DEFAULT: u32 = 1;

    fn new(env_context: Rc<FdkEnv>) -> Self {
        Self { env_context }
    }

    fn get_log_threshold(&self) -> u32 {
        self.env_context
            .fdk_log_threshold
            .as_ref()
            .map_or(Self::FDK_LOG_DEFAULT, |env| {
                env.parse::<u32>().unwrap_or(Self::FDK_LOG_DEFAULT)
            })
    }

    fn log(&self, content: &str, log_level: Option<u32>) {
        if log_level.unwrap_or(Self::FDK_LOG_DEFAULT) >= self.get_log_threshold() {
            eprintln!("{}", content);
        }
    }

    fn log_error(&self, err: FdkError) {
        self.log(&err.message, None);
        self.log(&err.backtrace.join("\n"), Some(Self::FDK_LOG_DEBUG))
    }

    fn debug(&self, content: &str) {
        self.log(content, Some(Self::FDK_LOG_DEBUG));
    }

    async fn handler() {
        unimplemented!();
    }
}

#[derive(Debug)]
struct FdkListener {
    socket_path: Rc<PathBuf>,
    private_socket_path: Rc<PathBuf>,
    private_socket: UnixListener,
    env: Rc<FdkEnv>,
    runner: FdkRunner,
}
impl FdkListener {
    const SOCKET_PREFIX: &'static str = "unix:/";
    const PRIVATE_SOCKET_SUFFIX: &'static str = ".private";

    fn new(env: Rc<FdkEnv>, runner: FdkRunner) -> RustFdkResult<Self> {
        Rc::clone(&env).fn_listener.as_ref().map_or(
            FdkError::new("No listener url provided").into(),
            |url| {
                if !url.starts_with(Self::SOCKET_PREFIX) {
                    FdkError::new("Listener url is not a unix socket").into()
                } else {
                    url.strip_prefix(Self::SOCKET_PREFIX).map_or(
                        FdkError::new("Cannot strip FdkListener.url prefix").into(),
                        |stripped_url| {
                            let private_socket_path = Rc::new(PathBuf::from(
                                [stripped_url, Self::PRIVATE_SOCKET_SUFFIX].concat(),
                            ));

                            Ok(Self {
                                socket_path: Rc::new(PathBuf::from(stripped_url)),
                                private_socket: UnixListener::bind(private_socket_path.as_path())?,
                                private_socket_path,
                                env,
                                runner,
                            })
                        },
                    )
                }
            },
        )
    }

    fn link_socket_file(&self) -> RustFdkResult<()> {
        File::open(self.private_socket_path.as_path()).map_or(
            FdkError::new("Cannot access private socket file").into(),
            |file| {
                file.set_permissions(Permissions::from_mode(0o666))?;
                symlink(
                    self.private_socket_path.as_path(),
                    self.socket_path.as_path(),
                )?;
                self.runner.debug(
                    &[
                        "Listening on ",
                        &self.private_socket_path.to_str().unwrap(),
                        "->",
                        &self.socket_path.to_str().unwrap(),
                    ]
                    .concat(),
                );
                Ok(())
            },
        )
    }
}

use actix_web::{App, HttpResponse, HttpServer};
use std::env::var;
use std::fs::{File, Permissions};
use std::os::unix::fs::{symlink, PermissionsExt};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::rc::Rc;
use actix_web::dev::{ServiceRequest, Service, ServiceResponse};
use actix_web::http::{HeaderValue, HeaderName};
use std::sync::Arc;

fn main() {
    let fdk_env: FdkEnv = FdkEnv {
        fn_listener: var("FN_LISTENER").ok(),
        fn_format: var("FN_FORMAT").ok(),
        fn_logframe_name: var("FN_LOGFRAME_NAME").ok(),
        fn_logframe_hdr: var("FN_LOGFRAME_HDR").ok(),
        fdk_log_threshold: var("FDK_LOG_THRESHOLD").ok(),
        fn_app_id: var("FN_APP_ID").ok(),
        fn_fn_id: var("FN_FN_ID").ok(),
        fn_memory: var("FN_MEMORY").ok(),
    };
    actix_web::rt::System::new("fdk-rust").block_on(actix_main(Rc::new(fdk_env)));
}

fn fdk_middleware_req(req: ServiceRequest, _env: Arc<FdkEnv>) -> ServiceRequest {
    req
}
fn fdk_middleware_res(res: ServiceResponse) -> ServiceResponse {
    let mut res = res;
    res.headers_mut().insert(
        HeaderName::from_static("Connection"),
        HeaderValue::from_static("close"),
    );
    res
}

async fn actix_main(env: Rc<FdkEnv>) {
    let runner = FdkRunner::new(Rc::clone(&env));
    let listener = FdkListener::new(Rc::clone(&env), runner).unwrap();

    let env = Arc::new(env.as_ref().clone());
    HttpServer::new(move || {
        let env = env.clone();

        App::new()
            .wrap_fn(move |req, srv| {
                let resp = srv.call(fdk_middleware_req(req, env.clone()));
                async {
                    Ok(fdk_middleware_res(resp.await?))
                }
            })
            .default_service(actix_web::web::to(HttpResponse::Ok))
    })
    .listen_uds(listener.private_socket)
    .unwrap()
    .run()
    .await
    .unwrap();
}
