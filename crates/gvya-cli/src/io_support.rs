//! Bounded CLI input/output support.

use super::*;

pub(super) fn print_json(value: &serde_json::Value) -> Result<(), String> {
    let mut writer = LimitedVecWriter::new(MACHINE_JSON_MAX_BYTES);
    match serde_json::to_writer(&mut writer, value) {
        Ok(()) => {}
        Err(_) if writer.exceeded => {
            return Err(format!(
                "machine JSON exceeds {} bytes",
                MACHINE_JSON_MAX_BYTES
            ));
        }
        Err(error) => return Err(format!("cannot serialize machine JSON: {error}")),
    }
    let mut stdout = io::stdout().lock();
    stdout
        .write_all(&writer.bytes)
        .map_err(|error| format!("cannot write machine JSON: {error}"))?;
    stdout
        .write_all(b"\n")
        .map_err(|error| format!("cannot finish machine JSON: {error}"))?;
    Ok(())
}

struct LimitedVecWriter {
    bytes: Vec<u8>,
    limit: usize,
    exceeded: bool,
}
impl LimitedVecWriter {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(limit.min(8 * 1024)),
            limit,
            exceeded: false,
        }
    }
}
impl Write for LimitedVecWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let Some(next) = self.bytes.len().checked_add(buf.len()) else {
            self.exceeded = true;
            return Err(io::Error::other("machine JSON byte limit exceeded"));
        };
        if next > self.limit {
            self.exceeded = true;
            return Err(io::Error::other("machine JSON byte limit exceeded"));
        }
        self.bytes.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn read_bounded_file(
    path: &Path,
    max_bytes: usize,
    label: &str,
) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path)
        .map_err(|error| format!("cannot open {} {}: {error}", label, path.display()))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("cannot stat {} {}: {error}", label, path.display()))?;
    if !metadata.is_file() {
        return Err(format!("{label} is not a regular file: {}", path.display()));
    }
    if metadata.len() > u64::try_from(max_bytes).unwrap_or(u64::MAX) {
        return Err(format!(
            "{label} exceeds {max_bytes} bytes: {}",
            path.display()
        ));
    }
    let read_limit = max_bytes
        .checked_add(1)
        .ok_or_else(|| format!("invalid {label} byte limit"))?;
    let capacity = usize::try_from(metadata.len())
        .unwrap_or(max_bytes)
        .min(max_bytes);
    let mut bytes = Vec::with_capacity(capacity);
    file.take(u64::try_from(read_limit).unwrap_or(u64::MAX))
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read {} {}: {error}", label, path.display()))?;
    if bytes.len() > max_bytes {
        return Err(format!(
            "{label} exceeded {max_bytes} bytes while reading: {}",
            path.display()
        ));
    }
    Ok(bytes)
}
