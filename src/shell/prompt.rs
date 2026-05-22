use std::io::Read;
use std::cell::RefCell;
use std::process::{ Command, Stdio };
use crate::config::Config;
use crate::shell::Environment;
use crate::shell::execute::Status;


pub struct Prompt {
    buf: String,
    status: Status,
    width: u16,
}

impl Default for Prompt {
    fn default() -> Self {
        Prompt {
            buf: "> ".into(),
            status: Status::BuiltIn(true),
            width: 1,
        }
    }
}

impl Prompt {
    pub fn set_status(&mut self, status: Status) {
        self.status = status;
    }

    pub fn set_width(&mut self, width: u16) {
        self.width = width;
    }

    pub fn as_str(&self) -> &str {
        self.buf.as_str()
    }
    
    pub fn update(&mut self, config: &Config, env: &RefCell<Environment>) {
        let Some(prompt) = &config.prompt
            else {
                return
            };

        let env = env.borrow();        
        let mut cmd = Command::new(&prompt.exe);
        cmd
            .current_dir(env.pwd())
            .envs(env.map.iter().map(|(k, v)| (k.as_os_str(), v.as_os_str())))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());

        for arg in &prompt.args {
            match arg.as_str() {
                "@status" => {
                    let status = self.status.code().to_string();
                    cmd.arg(status);
                },
                "@width" => {
                    let width = self.width.to_string();
                    cmd.arg(width);
                },
                _ => {
                    cmd.arg(arg);
                }
            }
        }

        if let Ok(child) = cmd.spawn() {
            let mut stdout = child.stdout.unwrap();
            self.buf.clear();
            let _ = stdout.read_to_string(&mut self.buf);
        }
    }
}
