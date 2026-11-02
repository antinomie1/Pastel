use pastel_ipc::{Client, Reply, Request, State, Workspace, decode, encode, socket_for};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;

#[test]
fn messages_are_single_json_lines() {
    let line = encode(&Request::Action("focus-workspace 2".into()));
    assert_eq!(line, "{\"Action\":\"focus-workspace 2\"}\n");
    assert_eq!(encode(&Request::Subscribe), "\"Subscribe\"\n");
    assert_eq!(
        decode::<Request>(&line).unwrap(),
        Request::Action("focus-workspace 2".into())
    );
    assert!(decode::<Request>("{\"Nope\":1}").is_err());
}

#[test]
fn socket_follows_the_wayland_display() {
    // SAFETY: no other test in this binary reads the environment.
    unsafe { std::env::set_var("XDG_RUNTIME_DIR", "/run/user/1") };
    let path = socket_for("wayland-1").unwrap();
    assert_eq!(path.to_str(), Some("/run/user/1/pastel.wayland-1.sock"));
}

#[test]
fn client_requests_and_subscribes() {
    let dir = std::env::temp_dir().join(format!("pastel-ipc-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("socket");
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).unwrap();
    let state = State {
        workspaces: vec![Workspace {
            id: 7,
            index: 1,
            output: "DP-1".into(),
            active: true,
            focused: true,
            windows: 0,
        }],
        ..State::default()
    };
    let sent = state.clone();
    let server = std::thread::spawn(move || {
        for _ in 0..2 {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let mut writer = stream;
            match decode::<Request>(&line).unwrap() {
                Request::Subscribe => {
                    for _ in 0..2 {
                        writer
                            .write_all(encode(&Reply::State(sent.clone())).as_bytes())
                            .unwrap();
                    }
                }
                _ => writer.write_all(encode(&Reply::Ok).as_bytes()).unwrap(),
            }
        }
    });
    let mut client = Client::connect_to(&path).unwrap();
    assert_eq!(
        client.request(&Request::FocusWorkspace(7)).unwrap(),
        Reply::Ok
    );
    let states: Vec<_> = Client::connect_to(&path)
        .unwrap()
        .subscribe()
        .unwrap()
        .collect();
    server.join().unwrap();
    assert_eq!(states.len(), 2);
    assert_eq!(states[1].as_ref().unwrap(), &state);
    let _ = std::fs::remove_dir_all(dir);
}
