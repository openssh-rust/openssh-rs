use super::{Result, Session};

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::str;

/// Build a [`Session`] with options.
#[derive(Debug, Clone)]
pub struct SessionBuilder {
    pub(crate) user: Option<String>,
    pub(crate) port: Option<String>,
    pub(crate) keyfile: Option<PathBuf>,
    pub(crate) connect_timeout: Option<String>,
    pub(crate) server_alive_interval: Option<u64>,
    pub(crate) known_hosts_check: KnownHosts,
    pub(crate) control_dir: Option<PathBuf>,
    pub(crate) config_file: Option<PathBuf>,
}

impl Default for SessionBuilder {
    fn default() -> Self {
        Self {
            user: None,
            port: None,
            keyfile: None,
            connect_timeout: None,
            server_alive_interval: None,
            known_hosts_check: KnownHosts::Add,
            control_dir: None,
            config_file: None,
        }
    }
}

impl SessionBuilder {
    /// Set the ssh user (`ssh -l`).
    ///
    /// Defaults to `None`.
    pub fn user(&mut self, user: String) -> &mut Self {
        self.user = Some(user);
        self
    }

    /// Set the port to connect on (`ssh -p`).
    ///
    /// Defaults to `None`.
    pub fn port(&mut self, port: u16) -> &mut Self {
        self.port = Some(format!("{}", port));
        self
    }

    /// Set the keyfile to use (`ssh -i`).
    ///
    /// Defaults to `None`.
    pub fn keyfile(&mut self, p: impl AsRef<Path>) -> &mut Self {
        self.keyfile = Some(p.as_ref().to_path_buf());
        self
    }

    /// See [`KnownHosts`].
    ///
    /// Default `KnownHosts::Add`.
    pub fn known_hosts_check(&mut self, k: KnownHosts) -> &mut Self {
        self.known_hosts_check = k;
        self
    }

    /// Set the connection timeout (`ssh -o ConnectTimeout`).
    ///
    /// This value is specified in seconds. Any sub-second duration remainder will be ignored.
    /// Defaults to `None`.
    pub fn connect_timeout(&mut self, d: std::time::Duration) -> &mut Self {
        self.connect_timeout = Some(d.as_secs().to_string());
        self
    }

    /// Set the timeout interval after which if no data has been received from the server, ssh
    /// will request a response from the server (`ssh -o ServerAliveInterval`).
    ///
    /// This value is specified in seconds. Any sub-second duration remainder will be ignored.
    /// Defaults to `None`.
    pub fn server_alive_interval(&mut self, d: std::time::Duration) -> &mut Self {
        self.server_alive_interval = Some(d.as_secs());
        self
    }

    /// Set the directory in which the temporary directory containing the control socket will
    /// be created.
    ///
    /// If not set, `./` will be used (the current directory).
    pub fn control_directory(&mut self, p: impl AsRef<Path>) -> &mut Self {
        self.control_dir = Some(p.as_ref().to_path_buf());
        self
    }

    /// Set an alternative per-user configuration file.
    ///
    /// By default, ssh uses `~/.ssh/config`. This is equivalent to `ssh -F <p>`.
    ///
    /// Defaults to `None`.
    pub fn config_file(&mut self, p: impl AsRef<Path>) -> &mut Self {
        self.config_file = Some(p.as_ref().to_path_buf());
        self
    }

    fn resolve<'a, 'b>(&'a self, mut destination: &'b str) -> (Cow<'a, Self>, &'b str) {
        // the "new" ssh://user@host:port form is not supported by all versions of ssh,
        // so we always translate it into the option form.
        let mut user = None;
        let mut port = None;
        if destination.starts_with("ssh://") {
            destination = &destination[6..];
            if let Some(at) = destination.find('@') {
                // specified a username -- extract it:
                user = Some(&destination[..at]);
                destination = &destination[(at + 1)..];
            }
            if let Some(colon) = destination.rfind(':') {
                let p = &destination[(colon + 1)..];
                if let Ok(p) = p.parse() {
                    // user specified a port -- extract it:
                    port = Some(p);
                    destination = &destination[..colon];
                }
            }
        }

        if user.is_none() && port.is_none() {
            return (Cow::Borrowed(self), destination);
        }

        let mut with_overrides = self.clone();
        if let Some(user) = user {
            with_overrides.user(user.to_owned());
        }

        if let Some(port) = port {
            with_overrides.port(port);
        }

        (Cow::Owned(with_overrides), destination)
    }

    /// Connect to the host at the given `host` over SSH.
    ///
    /// The format of `destination` is the same as the `destination` argument to `ssh`. It may be
    /// specified as either `[user@]hostname` or a URI of the form `ssh://[user@]hostname[:port]`.
    /// A username or port that is specified in the connection string overrides the one set in the
    /// builder (but does not change the builder).
    ///
    /// If connecting requires interactive authentication based on `STDIN` (such as reading a
    /// password), the connection will fail. Consider setting up keypair-based authentication
    /// instead.
    pub async fn connect<S: AsRef<str>>(&self, destination: S) -> Result<Session> {
        let destination = destination.as_ref();
        let (builder, destination) = self.resolve(destination);
        builder.just_connect(destination).await
    }

    pub(crate) async fn just_connect<S: AsRef<str>>(&self, host: S) -> Result<Session> {
        Ok(Session::new_process_imp(
            super::process_impl::builder::just_connect(self, host).await?,
        ))
    }

    /// Connect to the host at the given `host` over SSH.
    ///
    /// The format of `destination` is the same as the `destination` argument to `ssh`. It may be
    /// specified as either `[user@]hostname` or a URI of the form `ssh://[user@]hostname[:port]`.
    /// A username or port that is specified in the connection string overrides the one set in the
    /// builder (but does not change the builder).
    ///
    /// If connecting requires interactive authentication based on `STDIN` (such as reading a
    /// password), the connection will fail. Consider setting up keypair-based authentication
    /// instead.
    #[cfg(feature = "mux_client")]
    pub async fn connect_mux<S: AsRef<str>>(&self, destination: S) -> Result<Session> {
        let destination = destination.as_ref();
        let (builder, destination) = self.resolve(destination);
        builder.just_connect_mux(destination).await
    }

    #[cfg(feature = "mux_client")]
    pub(crate) async fn just_connect_mux<S: AsRef<str>>(&self, host: S) -> Result<Session> {
        Ok(Session::new_mux_client_imp(
            super::mux_client_impl::builder::just_connect(self, host).await?,
        ))
    }
}

/// Specifies how the host's key fingerprint should be handled.
#[derive(Debug, Clone)]
pub enum KnownHosts {
    /// The host's fingerprint must match what is in the known hosts file.
    ///
    /// If the host is not in the known hosts file, the connection is rejected.
    ///
    /// This corresponds to `ssh -o StrictHostKeyChecking=yes`.
    Strict,
    /// Strict, but if the host is not already in the known hosts file, it will be added.
    ///
    /// This corresponds to `ssh -o StrictHostKeyChecking=accept-new`.
    Add,
    /// Accept whatever key the server provides and add it to the known hosts file.
    ///
    /// This corresponds to `ssh -o StrictHostKeyChecking=no`.
    Accept,
}

impl KnownHosts {
    pub(crate) fn as_option(&self) -> &'static str {
        match *self {
            KnownHosts::Strict => "StrictHostKeyChecking=yes",
            KnownHosts::Add => "StrictHostKeyChecking=accept-new",
            KnownHosts::Accept => "StrictHostKeyChecking=no",
        }
    }
}

#[test]
fn resolve() {
    let b = SessionBuilder::default();
    let (b, d) = b.resolve("ssh://test-user@127.0.0.1:2222");
    assert_eq!(b.port.as_deref(), Some("2222"));
    assert_eq!(b.user.as_deref(), Some("test-user"));
    assert_eq!(d, "127.0.0.1");

    let b = SessionBuilder::default();
    let (b, d) = b.resolve("ssh://test-user@opensshtest:2222");
    assert_eq!(b.port.as_deref(), Some("2222"));
    assert_eq!(b.user.as_deref(), Some("test-user"));
    assert_eq!(d, "opensshtest");

    let b = SessionBuilder::default();
    let (b, d) = b.resolve("ssh://opensshtest:2222");
    assert_eq!(b.port.as_deref(), Some("2222"));
    assert_eq!(b.user.as_deref(), None);
    assert_eq!(d, "opensshtest");

    let b = SessionBuilder::default();
    let (b, d) = b.resolve("ssh://test-user@opensshtest");
    assert_eq!(b.port.as_deref(), None);
    assert_eq!(b.user.as_deref(), Some("test-user"));
    assert_eq!(d, "opensshtest");

    let b = SessionBuilder::default();
    let (b, d) = b.resolve("ssh://opensshtest");
    assert_eq!(b.port.as_deref(), None);
    assert_eq!(b.user.as_deref(), None);
    assert_eq!(d, "opensshtest");

    let b = SessionBuilder::default();
    let (b, d) = b.resolve("opensshtest");
    assert_eq!(b.port.as_deref(), None);
    assert_eq!(b.user.as_deref(), None);
    assert_eq!(d, "opensshtest");
}
