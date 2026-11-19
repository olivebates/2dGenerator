use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    Ask,
    Enhance,
    Code,
    NoCode,
}
impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ask => "Ask me questions",
            Self::Enhance => "Enhance",
            Self::Code => "Code",
            Self::NoCode => "No code",
        }
    }
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Ask => "Ask me questions about it if there are any. Dont code anything yet.",
            Self::Enhance => "How can I make this more satisfying? Don't code anything yet.",
            Self::Code => "Go ahead and code it.",
            Self::NoCode => "Dont code anything yet.",
        }
    }
    pub fn prompt(self, input: &str) -> String {
        format!("{}\n\n{}", input.trim(), self.suffix())
    }
    pub fn policy(self, folder: &str) -> Value {
        if self == Self::Code {
            json!({"type":"workspaceWrite","writableRoots":[folder],"networkAccess":false,"excludeTmpdirEnvVar":true,"excludeSlashTmp":true})
        } else {
            json!({"type":"readOnly","networkAccess":false})
        }
    }
    pub fn approval_policy(self) -> &'static str {
        if self == Self::Code {
            "on-request"
        } else {
            "never"
        }
    }
    pub fn reviewer(self) -> &'static str {
        if self == Self::Code {
            "auto_review"
        } else {
            "user"
        }
    }
}
#[derive(Default, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub text: String,
    #[serde(default)]
    pub item: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<String>,
}
#[derive(Default, Serialize, Deserialize)]
pub struct Chat {
    pub title: String,
    pub folder: String,
    pub thread: Option<String>,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub draft: String,
}
#[derive(Default, Serialize, Deserialize)]
pub struct Store {
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub reasoning: String,
    pub chats: Vec<Chat>,
    pub selected: usize,
    pub folder: String,
}
impl Store {
    pub fn load(path: &Path) -> Result<Self, String> {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| {
                format!(
                    "Cannot read saved chats: {e}. Original file preserved at {}",
                    path.display()
                )
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.to_string()),
        }
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let parent = path.parent().ok_or("Invalid storage path")?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        temp.write_all(&serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        temp.as_file().sync_all().map_err(|e| e.to_string())?;
        temp.persist(path).map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn new_chat(&mut self) {
        self.chats.push(Chat {
            title: "New chat".into(),
            folder: self.folder.clone(),
            ..Default::default()
        });
        self.selected = self.chats.len() - 1;
    }
}
pub fn storage_path() -> PathBuf {
    directories::ProjectDirs::from("", "Sepia", "Sepia Chat")
        .map(|p| p.data_local_dir().join("chats.json"))
        .unwrap_or_else(|| "chats.json".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn modes_enforce_read_only_and_exact_suffixes() {
        for mode in [Mode::Ask, Mode::Enhance, Mode::NoCode] {
            assert_eq!(mode.policy("C:/project")["type"], "readOnly");
        }
        assert_eq!(
            Mode::Code.policy("C:/project")["writableRoots"],
            json!(["C:/project"])
        );
        assert_eq!(
            Mode::Ask.prompt(" Idea "),
            "Idea\n\nAsk me questions about it if there are any. Dont code anything yet."
        );
        assert_eq!(
            Mode::Enhance.prompt("Idea"),
            "Idea\n\nHow can I make this more satisfying? Don't code anything yet."
        );
        assert_eq!(Mode::Code.prompt("Idea"), "Idea\n\nGo ahead and code it.");
        assert_eq!(
            Mode::NoCode.prompt("Idea"),
            "Idea\n\nDont code anything yet."
        );
    }
    #[test]
    fn chats_keep_their_folder_and_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chats.json");
        let mut store = Store {
            folder: "A".into(),
            model: "chosen-model".into(),
            reasoning: "high".into(),
            ..Default::default()
        };
        store.new_chat();
        store.chats[0].thread = Some("thread-1".into());
        store.folder = "B".into();
        store.new_chat();
        store.save(&path).unwrap();
        store.save(&path).unwrap();
        let loaded = Store::load(&path).unwrap();
        assert_eq!(loaded.model, "chosen-model");
        assert_eq!(loaded.reasoning, "high");
        assert_eq!(loaded.chats[0].folder, "A");
        assert_eq!(loaded.chats[1].folder, "B");
        assert_eq!(loaded.chats[0].thread.as_deref(), Some("thread-1"));
    }
    #[test]
    fn corrupt_storage_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chats.json");
        std::fs::write(&path, "bad json").unwrap();
        assert!(Store::load(&path).is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "bad json");
    }
}
