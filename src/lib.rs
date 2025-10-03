use std::{fs::{self, File}, io::{BufRead as _, BufReader, Read as _, Seek as _, SeekFrom, Write}, path::PathBuf, process::{Command, Stdio}, thread};

const HOYO_HPATCHZ_EXE: &[u8] = include_bytes!("./hpatchz/hpatchz_4.6.9.exe");
const KURO_HPATCHZ_EXE: &[u8] = include_bytes!("./hpatchz/hpatchz_4.8.0.exe");

pub struct HPatchz {
  pub e_type: HPatchzType,
  pub extracted_path: PathBuf,
  pub custom_args: Vec<String>,
}

pub enum HPatchzType {
  Hoyo,
  Kuro,
}

fn allowed_print(line: &str) -> bool {
  !line.trim().is_empty()
    && (line.contains("Patch inited")
    || line.contains("begin patch file")
    || line.contains("end patch file"))
}

impl HPatchz {
  pub fn new(e_type: HPatchzType, args: Vec<String>) -> Self {
    let temp_path = std::env::temp_dir().join("hpatchz.tmp");
    if !temp_path.exists() {
      let mut temp_file = File::create(&temp_path).expect("[hpatchz] Error creating temp file!");
      temp_file.write_all(match e_type {
        HPatchzType::Hoyo => HOYO_HPATCHZ_EXE,
        HPatchzType::Kuro => KURO_HPATCHZ_EXE,
      }).expect("[hpatchz] Error writing temp file data!");
      temp_file.flush().unwrap();
      drop(temp_file);
    }

    HPatchz {
      e_type: e_type,
      extracted_path: temp_path,
      custom_args: args,
    }
  }

  pub fn drop(&self) {
    fs::remove_file(&self.extracted_path).expect("[hpatchz] Unable to delete temp file!");
  }

  pub fn patch(&self, src_path: &PathBuf, dest_path: &PathBuf, diff_file: &PathBuf) -> i32 {
    let mut args: Vec<String> = Vec::new();

    args.push(src_path.to_string_lossy().to_string());
    args.push(diff_file.to_string_lossy().to_string());
    args.push(dest_path.to_string_lossy().to_string());

    args.push("-f".to_string());

    // custom args
    args.extend(self.custom_args.iter().cloned());

    tracing::debug!("[hpatchz] With args: {:?}", args);

    let mut child = Command::new(&self.extracted_path)
      .args(&args)
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .spawn()
      .expect("[hpatchz] Unable to run hpatchz!");

    if let Some(stdout) = child.stdout.take() {
      thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
          let line = line.unwrap();
          let trimmed = line.trim();

          if !trimmed.is_empty() && allowed_print(trimmed) {
            tracing::info!("[hpatchz] out : {}", trimmed);
          }
        }
      });
    }

    if let Some(stderr) = child.stderr.take() {
      thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
          let line = line.unwrap();
          tracing::warn!("[hpatchz] err: {}", line);
        }
      });
    }
    
    let status = child.wait().expect("[hpatchz] Unable to wait process to complete!");
    tracing::debug!("[hpatchz] Exit status: {:?}", status);

    match status.code() {
      Some(c) => c,
      None => {
        tracing::debug!("[hpatchz] Was killed by signal!");
        -99
      },
    }
  }

  pub fn patch_legacy(&self, src: &str, dest: &str, diff: &str) -> i32 {
    self.patch(&PathBuf::from(src), &PathBuf::from(dest), &PathBuf::from(diff))
  }
  
  pub fn patch_offset(&self, src_path: &PathBuf, dest_path: &PathBuf, diff_file: &PathBuf, start_offset: u64, patch_size: u64) -> i32 {
    let mut ldiff = File::open(diff_file).unwrap();
    ldiff.seek(SeekFrom::Start(start_offset)).unwrap(); // seek to start offset

    tracing::debug!("[hpatchz] Init new patch for offset {} - {} from {}", start_offset, patch_size, diff_file.display());

    // init buffer
    let mut ldiff_buffer = vec![0u8; patch_size as usize];
    let ldiff_bytes = ldiff.read(&mut ldiff_buffer).unwrap();

    // init filename
    let diff_file_name = diff_file.file_name().unwrap().to_str().unwrap().to_string();
    let ldiff_temp_path = dest_path.join(format!("{}_{}_{}.diff", diff_file_name, start_offset, patch_size));

    // start writing diff with offset to temp file
    let mut ldiff_temp_file = File::create(&ldiff_temp_path).expect("[hpatchz] Unable to create diff patch offset!");
    ldiff_temp_file.write_all(&ldiff_buffer[..ldiff_bytes]).expect("[hpatchz] Unable to write diff patch offset!");
    drop(ldiff_temp_file);

    tracing::debug!("[hpatchz] Diff patch is writen to {}", ldiff_temp_path.display());

    let retcode = self.patch(src_path, dest_path, &ldiff_temp_path);

    // delete temp file
    tracing::debug!("[hpatchz] Removing {}", ldiff_temp_path.display());
    fs::remove_file(ldiff_temp_path).expect("[hpatchz] Unable to delete diff patch offset!");

    retcode
  }
}