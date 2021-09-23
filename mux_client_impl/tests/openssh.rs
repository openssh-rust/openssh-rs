use std::io;
use std::io::Write;
use std::str;

use once_cell::sync::OnceCell;

use regex::Regex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use mux_client_impl::*;

// TODO: how do we test the connection actually _failing_ so that the master reports an error?

fn addr() -> String {
    std::env::var("TEST_HOST").unwrap_or("ssh://test-user@127.0.0.1:2222".to_string())
}

#[tokio::test]
#[cfg_attr(not(ci), ignore)]
async fn it_connects() {
    let session = Session::connect(&addr(), KnownHosts::Accept).await.unwrap();
    session.check().await.unwrap();
    session.close().await.unwrap();
}

#[tokio::test]
#[cfg_attr(not(ci), ignore)]
async fn control_dir() {
    let dirname = std::path::Path::new("control-test");
    assert!(!dirname.exists());
    std::fs::create_dir(dirname).unwrap();
    let session = SessionBuilder::default()
        .control_directory(&dirname)
        .connect(&addr())
        .await
        .unwrap();
    session.check().await.unwrap();
    let mut iter = std::fs::read_dir(&dirname).unwrap();
    assert!(iter.next().is_some());
    session.close().await.unwrap();
    std::fs::remove_dir(&dirname).unwrap();
}

#[derive(Default, Debug, PartialEq, Eq)]
struct ProtoUserHostPort<'a> {
    proto: Option<&'a str>,
    user: Option<&'a str>,
    host: Option<&'a str>,
    port: Option<&'a str>,
}

fn parse_user_host_port<'a>(s: &'a str) -> Option<ProtoUserHostPort> {
    static SSH_REGEX: OnceCell<Regex> = OnceCell::new();

    let ssh_regex = SSH_REGEX.get_or_init(|| {
        Regex::new(
            r#"(?x)^((?P<proto>[[:alpha:]]+)://)?((?P<user>.*?)@)?(?P<host>.*?)(:(?P<port>\d+))?$"#,
        )
        .unwrap()
    });

    ssh_regex.captures(s).and_then(|cap| {
        Some(ProtoUserHostPort {
            proto: cap.name("proto").and_then(|m| Some(m.as_str())),
            user: cap.name("user").and_then(|m| Some(m.as_str())),
            host: cap.name("host").and_then(|m| Some(m.as_str())),
            port: cap.name("port").and_then(|m| Some(m.as_str())),
        })
    })
}

#[test]
fn test_parse_proto_user_host_port() {
    let addr = "ssh://test-user@127.0.0.1:2222";
    let parsed_addr = parse_user_host_port(&addr).unwrap();
    assert_eq!("ssh", parsed_addr.proto.unwrap());
    assert_eq!("test-user", parsed_addr.user.unwrap());
    assert_eq!("127.0.0.1", parsed_addr.host.unwrap());
    assert_eq!("2222", parsed_addr.port.unwrap());
}

#[test]
fn test_parse_user_host_port() {
    let addr = "test-user@127.0.0.1:2222";
    let parsed_addr = parse_user_host_port(&addr).unwrap();
    assert!(parsed_addr.proto.is_none());
    assert_eq!("test-user", parsed_addr.user.unwrap());
    assert_eq!("127.0.0.1", parsed_addr.host.unwrap());
    assert_eq!("2222", parsed_addr.port.unwrap());
}

#[test]
fn test_parse_user_host() {
    let addr = "test-user@127.0.0.1";
    let parsed_addr = parse_user_host_port(&addr).unwrap();
    assert!(parsed_addr.proto.is_none());
    assert_eq!("test-user", parsed_addr.user.unwrap());
    assert_eq!("127.0.0.1", parsed_addr.host.unwrap());
    assert!(parsed_addr.port.is_none());
}

#[test]
fn test_parse_host_port() {
    let addr = "127.0.0.1:2222";
    let parsed_addr = parse_user_host_port(&addr).unwrap();
    assert!(parsed_addr.proto.is_none());
    assert!(parsed_addr.user.is_none());
    assert_eq!("127.0.0.1", parsed_addr.host.unwrap());
    assert_eq!("2222", parsed_addr.port.unwrap());
}

#[test]
fn test_parse_host() {
    let addr = "127.0.0.1";
    let parsed_addr = parse_user_host_port(&addr).unwrap();
    assert!(parsed_addr.proto.is_none());
    assert!(parsed_addr.user.is_none());
    assert_eq!("127.0.0.1", parsed_addr.host.unwrap());
    assert!(parsed_addr.port.is_none());
}

#[tokio::test]
#[cfg_attr(not(ci), ignore)]
async fn config_file() {
    let dirname = std::path::Path::new("config-file-test");
    let ssh_config_file = dirname.join("alternate_ssh_config");
    assert!(!dirname.exists());
    assert!(!ssh_config_file.exists());
    std::fs::create_dir(dirname).unwrap();

    let addr = addr();
    let parsed_addr = parse_user_host_port(&addr).unwrap();
    let ssh_config_contents = format!(
        r#"Host config-file-test
        User {}
        HostName {}
        Port {}"#,
        parsed_addr.user.unwrap_or("test-user"),
        parsed_addr.host.unwrap_or("127.0.0.1"),
        parsed_addr.port.unwrap_or("2222")
    );
    let mut ssh_config_handle = std::fs::File::create(&ssh_config_file).unwrap();
    ssh_config_handle
        .write_all(ssh_config_contents.as_bytes())
        .unwrap();
    let session = SessionBuilder::default()
        .known_hosts_check(KnownHosts::Accept)
        .config_file(&ssh_config_file)
        .connect("config-file-test") // this host name is resolved by the custom ssh_config.
        .await
        .unwrap();
    session.check().await.unwrap();
    session.close().await.unwrap();
    std::fs::remove_dir_all(&dirname).unwrap();
}

#[tokio::test]
#[cfg_attr(not(ci), ignore)]
async fn terminate_on_drop() {
    drop(Session::connect(&addr(), KnownHosts::Add).await.unwrap());
    // NOTE: how do we test that it actually killed the master here?
}

#[tokio::test]
#[cfg_attr(not(ci), ignore)]
async fn stdout() {
    let session = Session::connect(&addr(), KnownHosts::Accept).await.unwrap();

    let child = session.command("echo").arg("foo").output().await.unwrap();
    assert_eq!(child.stdout, b"foo\n");

    let child = session
        .command("echo")
        .arg("foo")
        .raw_arg(">")
        .arg("/dev/stderr")
        .output()
        .await
        .unwrap();
    assert!(child.stdout.is_empty());

    session.close().await.unwrap();
}

#[tokio::test]
#[cfg_attr(not(ci), ignore)]
async fn shell() {
    let session = Session::connect(&addr(), KnownHosts::Accept).await.unwrap();

    let child = session.shell("echo $USER").output().await.unwrap();
    assert_eq!(child.stdout, b"test-user\n");

    let child = session
        .shell(r#"touch "$USER Documents""#)
        .status()
        .await
        .unwrap();
    assert!(child.success());

    let child = session
        .shell(r#"rm "test-user Documents""#)
        .output()
        .await
        .unwrap();
    eprintln!("shell: {:#?}", child);
    assert!(child.status.success());

    let child = session.shell("echo '\\$SHELL'").output().await.unwrap();
    assert_eq!(str::from_utf8(&child.stdout).unwrap(), "$SHELL\n");

    let child = session
        .shell(r#"echo $USER | grep -c test"#)
        .status()
        .await
        .unwrap();
    eprintln!("shell: {:#?}", child);
    assert!(child.success());

    session.close().await.unwrap();
}

#[tokio::test]
#[cfg_attr(not(ci), ignore)]
async fn stderr() {
    let session = Session::connect(&addr(), KnownHosts::Accept).await.unwrap();

    let child = session.command("echo").arg("foo").output().await.unwrap();
    assert!(child.stderr.is_empty());

    let child = session
        .command("echo")
        .arg("foo")
        .raw_arg(">")
        .arg("/dev/stderr")
        .output()
        .await
        .unwrap();
    assert_eq!(child.stderr, b"foo\n");

    session.close().await.unwrap();
}

#[tokio::test]
#[cfg_attr(not(ci), ignore)]
async fn stdin() {
    let session = Session::connect(&addr(), KnownHosts::Accept).await.unwrap();

    let mut child = session
        .command("cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .await
        .unwrap();

    // write something to standard in and send EOF
    let mut stdin = child.stdin().take().unwrap();
    stdin.write_all(b"hello world").await.unwrap();
    drop(stdin);

    // cat should print it back on stdout
    let mut stdout = child.stdout().take().unwrap();
    let mut out = String::new();
    stdout.read_to_string(&mut out).await.unwrap();
    assert_eq!(out, "hello world");
    drop(stdout);

    // cat should now have terminated
    let status = child.wait().await.unwrap();
    drop(child);

    // ... successfully
    assert!(status.success());

    session.close().await.unwrap();
}

#[tokio::test]
#[cfg_attr(not(ci), ignore)]
async fn bad_remote_command() {
    let session = Session::connect(&addr(), KnownHosts::Accept).await.unwrap();

    // a bad remote command should result in a _local_ error.
    let failed = session.command("no such program").output().await.unwrap();
    eprintln!("{:?}", failed);
    assert!(!failed.status.success());

    // no matter how you run it
    let failed = session.command("no such program").status().await.unwrap();
    eprintln!("{:?}", failed);
    assert!(!failed.success());

    // even if you spawn first
    let mut child = session.command("no such program").spawn().await.unwrap();
    let failed = child.wait().await.unwrap();
    eprintln!("{:?}", failed);
    assert!(!failed.success());
    child.disconnect().await.unwrap();

    // of if you want output
    let child = session.command("no such program").spawn().await.unwrap();
    let failed = child.wait_with_output().await.unwrap();
    eprintln!("{:?}", failed);
    assert!(!failed.status.success());
}

#[tokio::test]
async fn connect_timeout() {
    use std::time::{Duration, Instant};

    let t = Instant::now();
    let mut sb = SessionBuilder::default();
    let failed = sb
        .connect_timeout(Duration::from_secs(1))
        .connect("192.0.0.8")
        .await
        .unwrap_err();
    assert!(t.elapsed() > Duration::from_secs(1));
    assert!(t.elapsed() < Duration::from_secs(2));
    eprintln!("{:?}", failed);
    assert!(matches!(failed, Error::Connect(ref e) if e.kind() == io::ErrorKind::TimedOut));
}

#[tokio::test]
#[cfg_attr(not(ci), ignore)]
async fn spawn_and_wait() {
    use std::time::{Duration, Instant};

    let session = Session::connect(&addr(), KnownHosts::Accept).await.unwrap();

    let t = Instant::now();
    let sleeping1 = session.command("sleep").arg("1").spawn().await.unwrap();
    let sleeping2 = sleeping1
        .session()
        .command("sleep")
        .arg("2")
        .spawn()
        .await
        .unwrap();
    sleeping1.wait_with_output().await.unwrap();
    assert!(t.elapsed() > Duration::from_secs(1));
    sleeping2.wait_with_output().await.unwrap();
    assert!(t.elapsed() > Duration::from_secs(2));

    session.close().await.unwrap();
}

#[tokio::test]
#[cfg_attr(not(ci), ignore)]
async fn escaping() {
    let session = Session::connect(&addr(), KnownHosts::Accept).await.unwrap();

    let status = dbg!(session
        .command("printf")
        .arg("%d %d")
        .arg("1")
        .arg("2")
        .output()
        .await
        .unwrap())
    .status;
    assert!(status.success());

    let status = dbg!(session
        .command("printf")
        .args(vec!["%d %d", "1", "2"])
        .output()
        .await
        .unwrap())
    .status;
    assert!(status.success());

    let status = dbg!(session
        .command("printf")
        .arg("%d %d")
        .raw_arg("1 2")
        .output()
        .await
        .unwrap())
    .status;
    assert!(status.success());

    let status = dbg!(session
        .command("printf")
        .arg("%d %d")
        .raw_args(std::iter::once("1 2"))
        .output()
        .await
        .unwrap())
    .status;
    assert!(status.success());

    let status = dbg!(session
        .raw_command("printf '%d %d'")
        .arg("1")
        .arg("2")
        .output()
        .await
        .unwrap())
    .status;
    assert!(status.success());

    session.close().await.unwrap();
}

#[tokio::test]
#[cfg_attr(not(ci), ignore)]
async fn process_exit_on_signal() {
    let session = Session::connect(&addr(), KnownHosts::Accept).await.unwrap();

    let mut sleeping = session.command("sleep").arg("5566").spawn().await.unwrap();

    // give it some time to make sure it starts
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // Now stop that process.
    //
    // We use `pkill -f` to match on the number rather than the `sleep` command, since other tests
    // may use `sleep`. We use `-o` to ensure that we don't accidentally kill the ssh connection
    // itself, but instead match the _oldest_ matching command.
    let killed = session
        .command("pkill")
        .arg("-f")
        .arg("-o")
        .arg("5566")
        .output()
        .await
        .unwrap();
    eprintln!("process_exit_on_signal: {:?}", killed);
    assert!(killed.status.success());

    // await that process — this will yield "Disconnected", since the remote process disappeared
    eprintln!("process_exit_on_signal: Waiting for sleeping to exit");
    let failed = sleeping.wait().await.unwrap();
    eprintln!("process_exit_on_signal: {:?}", failed);
    assert!(!failed.success());

    // the connection should still work though
    let _ = session.check().await.unwrap();
}

// broken_connection is comment out because mux_client_impl does not
// utilize external process to communicate to the ssh multiplex server/master.
//
//#[tokio::test]
//#[cfg_attr(not(ci), ignore)]
//async fn broken_connection() { // Run for over 60 seconds
//    let session = Session::connect(&addr(), KnownHosts::Accept).await.unwrap();
//
//    let sleeping = session.command("sleep").arg("1000").spawn().await.unwrap();
//
//    // get ID of remote ssh process
//    let ppid = session
//        .command("echo")
//        .raw_arg("$PPID")
//        .output()
//        .await
//        .unwrap();
//    eprintln!("ppid: {:?}", ppid);
//    let ppid: u32 = String::from_utf8(ppid.stdout)
//        .unwrap()
//        .trim()
//        .parse()
//        .unwrap(); // TODO: This failed: ParseIntError { kind: Empty }
//
//    // and kill it -- this kills the master connection
//    let killed = session
//        .command("kill")
//        .arg("-9")
//        .arg(&format!("{}", ppid))
//        .output()
//        .await
//        .unwrap_err();
//    eprintln!("{:?}", killed);
//    assert!(matches!(killed, Error::Disconnected));
//
//    // this fails because the master connection is gone
//    let failed = session
//        .command("echo")
//        .arg("foo")
//        .output()
//        .await
//        .unwrap_err();
//    eprintln!("{:?}", failed);
//    assert!(matches!(failed, Error::Disconnected));
//
//    // so does this
//    let failed = session
//        .command("echo")
//        .arg("foo")
//        .status()
//        .await
//        .unwrap_err();
//    eprintln!("{:?}", failed);
//    assert!(matches!(failed, Error::Disconnected));
//
//    // the spawned child we're waiting for must also have failed
//    let failed = sleeping.wait_with_output().await.unwrap_err();
//    eprintln!("{:?}", failed);
//    assert!(matches!(failed, Error::Disconnected));
//
//    // check should obviously fail
//    let failed = session.check().await.unwrap_err();
//    if let Error::Master(ref ioe) = failed {
//        if ioe.kind() != io::ErrorKind::ConnectionAborted {
//            eprintln!("{:?}", ioe);
//            assert_eq!(ioe.kind(), io::ErrorKind::ConnectionAborted);
//        }
//    } else {
//        unreachable!("{:?}", failed);
//    }
//
//    // what should close do in this instance?
//    // probably not return an error, since the connection _is_ closed.
//    session.close().await.unwrap();
//}

#[tokio::test]
async fn cannot_resolve() {
    match Session::connect("bad-host", KnownHosts::Accept)
        .await
        .unwrap_err()
    {
        Error::Connect(e) => {
            eprintln!("{:?}", e);
            assert_eq!(e.kind(), io::ErrorKind::Other);
        }
        e => unreachable!("{:?}", e),
    }
}

#[tokio::test]
async fn no_route() {
    match Session::connect("255.255.255.255", KnownHosts::Accept)
        .await
        .unwrap_err()
    {
        Error::Connect(e) => {
            eprintln!("{:?}", e);
            assert_eq!(e.kind(), io::ErrorKind::Other);
        }
        e => unreachable!("{:?}", e),
    }
}

#[tokio::test]
async fn connection_refused() {
    match Session::connect("ssh://127.0.0.1:9", KnownHosts::Accept)
        .await
        .unwrap_err()
    {
        Error::Connect(e) => {
            eprintln!("{:?}", e);
            assert_eq!(e.kind(), io::ErrorKind::ConnectionRefused);
        }
        e => unreachable!("{:?}", e),
    }
}

#[tokio::test]
async fn auth_failed() {
    let addr = if cfg!(ci) {
        // prefer the known-accessible test server when available
        addr().replace("test-user", "bad-user")
    } else {
        String::from("ssh://openssh-tester@login.csail.mit.edu")
    };

    match Session::connect(&addr, KnownHosts::Accept)
        .await
        .unwrap_err()
    {
        Error::Connect(e) => {
            eprintln!("{:?}", e);
            assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
        }
        e => unreachable!("{:?}", e),
    }
}
