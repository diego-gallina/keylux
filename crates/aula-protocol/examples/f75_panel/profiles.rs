use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::Path};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Profile {
    pub name: String,
    pub color: String,
    pub brightness: u8,
    pub seconds: u16,
    pub continuous: bool,
}

pub fn parse_hex(hex: &str) -> Option<[u8; 3]> {
    let hex = hex.trim().trim_start_matches('#');
    if hex.len() != 6 || !hex.bytes().all(|n| n.is_ascii_hexdigit()) {
        return None;
    }
    let rgb = u32::from_str_radix(hex, 16).ok()?;
    Some([(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8])
}

pub fn scale(rgb: [u8; 3], brightness: u8) -> [u8; 3] {
    rgb.map(|c| ((u16::from(c) * u16::from(brightness.min(100)) + 50) / 100) as u8)
}

fn valid(p: &Profile) -> bool {
    let mut chars = p.name.chars();
    p.name == p.name.trim()
        && p.name.chars().count() <= 30
        && chars.next().is_some_and(|c| c.is_alphanumeric())
        && chars.all(|c| c.is_alphanumeric() || matches!(c, ' ' | '_' | '-'))
        && p.brightness <= 100
        && (1..=3600).contains(&p.seconds)
        && parse_hex(&p.color).is_some()
}

pub fn load(path: &Path) -> anyhow::Result<Vec<Profile>> {
    let metadata = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e.into()),
    };
    anyhow::ensure!(
        metadata.len() <= 16384,
        "Arquivo de perfis maior que o limite."
    );
    let bytes = fs::read(path)?;
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
    let records: Vec<Profile> = serde_json::from_slice(bytes)?;
    anyhow::ensure!(
        records.len() <= 20 && records.iter().all(valid),
        "Arquivo de perfis inválido."
    );
    for (i, record) in records.iter().enumerate() {
        anyhow::ensure!(
            !records[..i]
                .iter()
                .any(|p| p.name.to_lowercase() == record.name.to_lowercase()),
            "Há nomes de perfis duplicados."
        );
    }
    Ok(records)
}

pub fn save(path: &Path, profile: Profile) -> anyhow::Result<()> {
    anyhow::ensure!(
        valid(&profile),
        "Nome ou valores inválidos. Digite um nome de até 30 caracteres."
    );
    let mut records = load(path)?;
    records.retain(|p| p.name.to_lowercase() != profile.name.to_lowercase());
    records.push(profile);
    anyhow::ensure!(records.len() <= 20, "Limite de 20 perfis.");
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Pasta de perfis indisponível."))?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!("profiles-{}.tmp", std::process::id()));
    let mut output = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    let result = (|| -> anyhow::Result<()> {
        output.write_all(&serde_json::to_vec(&records)?)?;
        output.sync_all()?;
        drop(output);
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn profile(name: &str, brightness: u8) -> Profile {
        Profile {
            name: name.into(),
            color: "00FFFF".into(),
            brightness,
            seconds: 10,
            continuous: false,
        }
    }

    #[test]
    fn preview_and_input_validation() {
        assert_eq!(scale(parse_hex("#00FFFF").unwrap(), 35), [0, 89, 89]);
        assert_eq!(parse_hex("00FFFF; bad"), None);
        assert!(!valid(&profile("../x", 35)));
        assert!(!valid(&profile("Noite", 101)));
    }

    #[test]
    fn profiles_save_on_demand_and_never_overwrite_corrupt_input() {
        let dir = std::env::temp_dir().join(format!("keylux-profile-test-{}", std::process::id()));
        let _guard = Cleanup(&dir);
        let path = dir.join("profiles.json");
        assert!(load(&path).unwrap().is_empty() && !dir.exists());
        save(&path, profile("Ciano", 35)).unwrap();
        save(&path, profile("Roxo", 44)).unwrap();
        save(&path, profile("CIANO", 55)).unwrap();
        assert_eq!(
            load(&path)
                .unwrap()
                .iter()
                .map(|p| p.brightness)
                .collect::<Vec<_>>(),
            [44, 55]
        );
        fs::write(&path, b"{bad").unwrap();
        assert!(save(&path, profile("Ciano", 99)).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"{bad");
    }

    struct Cleanup<'a>(&'a Path);
    impl Drop for Cleanup<'_> {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(self.0);
        }
    }
}
