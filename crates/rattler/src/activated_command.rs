use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::Stdio;
use tokio::io;
use tokio::process::Child;

pub struct ActivatedCommand {
    program: OsString,
    args: Vec<OsString>,
    env: HashMap<OsString, OsString>,
    stdin: Option<Stdio>,
    stdout: Option<Stdio>,
    stderr: Option<Stdio>,
}

impl ActivatedCommand {
    pub fn new(program: impl AsRef<OsStr>) -> Self {
        Self {
            program: program.as_ref().to_owned(),
            args: Vec::new(),
            env: HashMap::new(),
            stdin: None,
            stdout: None,
            stderr: None,
        }
    }

    pub fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
        self.args.push(arg.as_ref().to_owned());
        self
    }

    pub fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        for arg in args {
            self.args.push(arg.as_ref().to_owned());
        }
        self
    }

    pub fn env<K, V>(&mut self, key: K, val: V) -> &mut Self
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.env
            .insert(key.as_ref().to_owned(), val.as_ref().to_owned());
        self
    }

    pub fn stdin<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Self {
        self.stdin = Some(cfg.into());
        self
    }

    pub fn stdout<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Self {
        self.stdout = Some(cfg.into());
        self
    }

    pub fn stderr<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Self {
        self.stderr = Some(cfg.into());
        self
    }

    pub fn spawn(&mut self, prefix: &Path) -> io::Result<Child> {
        let mut command = tokio::process::Command::new(&self.program);

        command.args(&self.args).envs(&self.env);

        if let Some(stdin) = std::mem::take(&mut self.stdin) {
            command.stdin(stdin);
        }
        if let Some(stdout) = std::mem::take(&mut self.stdout) {
            command.stdout(stdout);
        }
        if let Some(stderr) = std::mem::take(&mut self.stderr) {
            command.stderr(stderr);
        }

        command.spawn()
    }
}
