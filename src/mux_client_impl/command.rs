use super::Error;
use super::RemoteChild;
use super::{as_raw_fd, Stdio};

use std::ffi::OsStr;
use std::path::PathBuf;

use openssh_mux_client::connection::{Connection, Session};

#[derive(Debug)]
pub(crate) struct Command {
    cmd: String,
    ctl: PathBuf,

    stdin_v: Stdio,
    stdout_v: Stdio,
    stderr_v: Stdio,
}

impl Command {
    pub(crate) fn new(ctl: PathBuf, cmd: String) -> Self {
        Self {
            cmd,
            ctl,

            stdin_v: Stdio::null(),
            stdout_v: Stdio::null(),
            stderr_v: Stdio::null(),
        }
    }

    pub(crate) fn arg<S: AsRef<str>>(&mut self, arg: S) -> &mut Self {
        self.cmd.push_str(" '");
        // TODO: Escape all `'` in `arg`
        self.cmd.push_str(arg.as_ref());
        self.cmd.push('\'');
        self
    }

    pub(crate) fn raw_arg<S: AsRef<OsStr>>(&mut self, arg: S) -> &mut Self {
        self.cmd.push(' ');
        self.cmd.push_str(&arg.as_ref().to_string_lossy());
        self
    }

    pub(crate) fn stdin<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Self {
        self.stdin_v = cfg.into();
        self
    }

    pub(crate) fn stdout<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Self {
        self.stdout_v = cfg.into();
        self
    }

    pub(crate) fn stderr<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Self {
        self.stderr_v = cfg.into();
        self
    }

    pub(crate) async fn spawn(&mut self) -> Result<RemoteChild, Error> {
        let (stdin, child_stdin) = self.stdin_v.into_stdin()?;
        let (stdout, child_stdout) = self.stdout_v.into_stdout()?;
        let (stderr, child_stderr) = self.stderr_v.into_stderr()?;

        let session = Session::builder().cmd(&self.cmd).build();

        let established_session = Connection::connect(&self.ctl)
            .await?
            .open_new_session(
                &session,
                &[as_raw_fd(&stdin)?, as_raw_fd(&stdout)?, as_raw_fd(&stderr)?],
            )
            .await?;

        Ok(RemoteChild::new(
            established_session,
            child_stdin,
            child_stdout,
            child_stderr,
        ))
    }
}
