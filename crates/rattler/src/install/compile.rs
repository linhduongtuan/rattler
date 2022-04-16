use crate::activated_command::ActivatedCommand;
use pretty_env_logger::init;
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::{io, panic};
use tempfile::{tempfile, NamedTempFile};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Child;
use tokio::select;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;
use tokio::task::{JoinError, JoinHandle};

/// An object that spawns a process which is used for compiling python source code to bytecode.
/// The compiler process is lazily started when a compilation request is received.
pub struct PythonCompiler {
    queue: UnboundedSender<CompilationRequest>,
    host_join_handle: JoinHandle<()>,
}

#[derive(Debug, Error)]
pub enum CompilationError {
    #[error("source path does not exist")]
    SourceDoesNotExist(#[source] io::Error),
}

impl PythonCompiler {
    /// Returns a new instance of a python compiler.
    pub fn new(prefix: &Path, python_path: &Path) -> Self {
        let (sender, receiver) = unbounded_channel();
        let join_handle = tokio::spawn(compiler_host(
            prefix.to_path_buf(),
            python_path.to_path_buf(),
            receiver,
        ));
        Self {
            queue: sender,
            host_join_handle: join_handle,
        }
    }

    /// Compile the python file at the specified `source` location and store the result in the
    /// `destination` path.
    pub async fn compile(&self, source: &Path) -> Result<PathBuf, CompilationError> {
        let (sender, receiver) = oneshot::channel();
        let request = CompilationRequest {
            source: source.to_path_buf(),
            sender,
        };
        self.queue
            .send(request)
            .expect("the compiler host shut down prematurely");
        receiver
            .await
            .expect("error receiving python compilation result")
    }
}

/// A compilation request send to the compiler task
#[derive(Debug)]
struct CompilationRequest {
    source: PathBuf,
    sender: oneshot::Sender<Result<PathBuf, CompilationError>>,
}

async fn compiler_host(
    prefix: PathBuf,
    python_path: PathBuf,
    mut requests: UnboundedReceiver<CompilationRequest>,
) {
    let request = if let Some(request) = requests.recv().await {
        request
    } else {
        return;
    };

    // Write the compilation host source code to a temporary file
    let mut compilation_source = NamedTempFile::new()
        .expect("could not create temporary file for python compilation sourcecode");
    compilation_source.write_all(include_bytes!("compile_pyc.py"));

    // Spawn the compilation host
    let child = ActivatedCommand::new(prefix.join(python_path))
        .arg("-Wi")
        .arg("-u")
        .arg(compilation_source.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn(&prefix)
        .expect("unable to spawn compilation host");

    // Get file handles
    let mut stdin = child.stdin.unwrap();
    let mut stdout = BufReader::new(child.stdout.unwrap()).lines();

    // Store requests with their response
    let mut filename_to_request = HashMap::new();

    // Send the initial request to the host
    let path_string = request.source.to_string_lossy().into_owned();
    filename_to_request.insert(path_string.clone(), request.sender);
    stdin
        .write_all(format!("{}\n", &path_string).as_bytes())
        .await
        .expect("could not write to python compiler host");

    loop {
        select! {
            Some(request) = requests.recv() => {
                let path_string = request.source.to_string_lossy().into_owned();
                filename_to_request.insert(path_string.clone(), request.sender);
                stdin
                    .write_all(format!("{}\n", &path_string).as_bytes())
                    .await
                    .expect("could not write to python compiler host");
                log::trace!("queuing '{}' for compilation", path_string);
            },
            Ok(Some(line)) = stdout.next_line() => {
                log::trace!("finished compiling '{}'", line);
                let receiver = filename_to_request.remove(&line).expect("could not find compilation response entry");
                receiver.send(Ok(PathBuf::new()));
            }
            else => break,
        }
    }
}
