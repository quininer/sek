#![cfg(unix)]

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use completest_pty::Term;
use ptyprocess::PtyProcess;
use serde_json::json;

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(name: &str) -> Result<Self> {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();
        let path =
            std::env::temp_dir().join(format!("sek-{name}-{}-{suffix:x}", std::process::id()));
        fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct PtySession {
    process: PtyProcess,
    writer: fs::File,
    parser: Arc<Mutex<vt100::Parser>>,
    reader: Option<JoinHandle<()>>,
}

impl PtySession {
    fn spawn(cwd: &Path, config: &Path, term: Term, home: &Path) -> Result<Self> {
        let bin = std::env::var_os("CARGO_BIN_EXE_seksh").context("missing CARGO_BIN_EXE_seksh")?;
        let cache_home = home.join("xdg-cache");
        let config_home = home.join("xdg-config");
        fs::create_dir_all(&cache_home)?;
        fs::create_dir_all(&config_home)?;

        let mut command = Command::new(bin);
        command
            .arg("-c")
            .arg(config)
            .current_dir(cwd)
            .env("HOME", home)
            .env("XDG_CACHE_HOME", &cache_home)
            .env("XDG_CONFIG_HOME", &config_home);

        let mut process = PtyProcess::spawn(command)?;
        process.set_window_size(term.get_width(), term.get_height())?;

        let parser = Arc::new(Mutex::new(vt100::Parser::new(
            term.get_height(),
            term.get_width(),
            0,
        )));
        let reader = process.get_raw_handle()?;
        let writer = process.get_raw_handle()?;
        let parser_bg = Arc::clone(&parser);

        let reader = thread::spawn(move || {
            let mut reader = reader;
            let mut buf = [0; 4096];

            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 {
                    break;
                }

                parser_bg.lock().unwrap().process(&buf[..n]);
            }
        });

        Ok(Self {
            process,
            writer,
            parser,
            reader: Some(reader),
        })
    }

    fn send(&mut self, input: &[u8]) -> Result<()> {
        self.writer.write_all(input)?;
        self.writer.flush()?;
        Ok(())
    }

    fn resize(&mut self, term: &Term) -> Result<()> {
        self.process
            .set_window_size(term.get_width(), term.get_height())?;
        self.parser
            .lock()
            .unwrap()
            .screen_mut()
            .set_size(term.get_height(), term.get_width());
        Ok(())
    }

    fn rows(&self) -> Vec<String> {
        let parser = self.parser.lock().unwrap();
        let (_, cols) = parser.screen().size();
        parser.screen().rows(0, cols).collect()
    }

    fn cursor_position(&self) -> (u16, u16) {
        self.parser.lock().unwrap().screen().cursor_position()
    }

    fn wait_for<F>(&self, timeout: Duration, mut predicate: F) -> Result<()>
    where
        F: FnMut(&Self) -> bool,
    {
        let start = SystemTime::now();
        while start.elapsed().unwrap_or_default() < timeout {
            if predicate(self) {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(20));
        }

        bail!(
            "timed out waiting for terminal state; rows={:?} cursor={:?}",
            self.rows(),
            self.cursor_position()
        )
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        let _ = self.process.exit(true);
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

fn write_executable(path: &Path, content: &str) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::write(path, content)?;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)?;
    Ok(())
}

fn write_prompt_config(root: &Path, prompt_script: &Path, prompt_args: &[&str]) -> Result<PathBuf> {
    let config = json!({
        "prompt": {
            "exe": prompt_script,
            "args": prompt_args,
        }
    })
    .to_string();
    let path = root.join("config.sh");
    write_executable(&path, &format!("#!/bin/sh\nprintf '%s' '{config}'\n"))?;
    Ok(path)
}

fn write_empty_config(root: &Path) -> Result<PathBuf> {
    let path = root.join("config.sh");
    write_executable(&path, "#!/bin/sh\nprintf '%s' '{}'\n")?;
    Ok(path)
}

fn visible_name_count(rows: &[String], names: &[String]) -> usize {
    let text = rows.join("\n");
    names
        .iter()
        .filter(|name| text.contains(name.as_str()))
        .count()
}

fn screen_text(rows: &[String]) -> String {
    rows.join("\n")
}

#[test]
fn prompt_refreshes_after_resize() -> Result<()> {
    let temp = TempDir::new("pr")?;
    let home = temp.path().join("home");
    let cwd = temp.path().join("cwd");
    fs::create_dir_all(&home)?;
    fs::create_dir_all(&cwd)?;
    let prompt = temp.path().join("prompt.sh");
    write_executable(&prompt, "#!/bin/sh\nprintf 'P%s' \"$1\"\n")?;
    let config = write_prompt_config(temp.path(), &prompt, &["@width"])?;

    let initial = Term::new().width(10).height(4);
    let mut session = PtySession::spawn(&cwd, &config, initial, &home)?;
    session.wait_for(Duration::from_secs(5), |session| {
        session
            .rows()
            .first()
            .is_some_and(|row| row.contains("P10"))
    })?;

    let resized = Term::new().width(20).height(4);
    session.resize(&resized)?;
    session.wait_for(Duration::from_secs(5), |session| {
        session
            .rows()
            .first()
            .is_some_and(|row| row.contains("P20"))
    })?;

    Ok(())
}

#[test]
fn path_selector_expands_visible_entries_after_resize() -> Result<()> {
    let temp = TempDir::new("ps")?;
    let home = temp.path().join("home");
    let cwd = temp.path().join("cwd");
    fs::create_dir_all(&home)?;
    fs::create_dir_all(&cwd)?;
    let config = write_empty_config(temp.path())?;

    let names = (0..8).map(|idx| format!("entry{idx}")).collect::<Vec<_>>();
    for name in &names {
        fs::write(cwd.join(name), name)?;
    }

    let initial = Term::new().width(40).height(5);
    let mut session = PtySession::spawn(&cwd, &config, initial, &home)?;
    session.send(b"echo ./\t")?;
    session.wait_for(Duration::from_secs(5), |session| {
        visible_name_count(&session.rows(), &names) >= 2
    })?;

    let initial_count = visible_name_count(&session.rows(), &names);

    let resized = Term::new().width(40).height(10);
    session.resize(&resized)?;
    session.wait_for(Duration::from_secs(5), |session| {
        visible_name_count(&session.rows(), &names) > initial_count
    })?;

    Ok(())
}

#[test]
fn path_selector_accepts_hidden_path_prefix_without_duplicate_suffix() -> Result<()> {
    let temp = TempDir::new("ph")?;
    let home = temp.path().join("home");
    let cwd = temp.path().join("cwd");
    fs::create_dir_all(&home)?;
    fs::create_dir_all(cwd.join("dir"))?;
    fs::write(cwd.join("dir").join(".config"), "config")?;
    let config = write_empty_config(temp.path())?;

    let initial = Term::new().width(60).height(8);
    let mut session = PtySession::spawn(&cwd, &config, initial, &home)?;
    session.send(b"cd ./dir/.c\t")?;
    session.wait_for(Duration::from_secs(5), |session| {
        screen_text(&session.rows()).contains(".config")
    })?;

    session.send(b"\r")?;
    session.wait_for(Duration::from_secs(5), |session| {
        screen_text(&session.rows()).contains("cd dir/.config")
    })?;

    assert!(!screen_text(&session.rows()).contains(".configconfig"));
    Ok(())
}

#[test]
fn path_selector_accepts_exact_file_path_without_directory_error() -> Result<()> {
    let temp = TempDir::new("pf")?;
    let home = temp.path().join("home");
    let cwd = temp.path().join("cwd");
    fs::create_dir_all(&home)?;
    fs::create_dir_all(&cwd)?;
    fs::write(cwd.join("entry0"), "entry0")?;
    let config = write_empty_config(temp.path())?;

    let initial = Term::new().width(60).height(8);
    let mut session = PtySession::spawn(&cwd, &config, initial, &home)?;
    session.send(b"cat ./entry0\t")?;
    session.wait_for(Duration::from_secs(5), |session| {
        let text = screen_text(&session.rows());
        text.contains("entry0") && !text.contains("Not a directory")
    })?;

    session.send(b"\r")?;
    session.wait_for(Duration::from_secs(5), |session| {
        screen_text(&session.rows()).contains("cat ./entry0")
    })?;

    assert!(!screen_text(&session.rows()).contains("Not a directory"));
    Ok(())
}
