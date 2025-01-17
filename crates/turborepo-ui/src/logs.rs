use std::{
    fs::File,
    io::{BufRead, BufReader, BufWriter, Write},
};

use tracing::{debug, warn};
use turbopath::AbsoluteSystemPath;

use crate::{prefixed::PrefixedUI, Error, PrefixedWriter};

/// Receives logs and multiplexes them to a log file and/or a prefixed
/// writer
pub struct LogWriter<W> {
    log_file: Option<BufWriter<File>>,
    prefixed_writer: Option<PrefixedWriter<W>>,
}

/// Derive didn't work here.
/// (we don't actually need `W` to implement `Default` here)
impl<W> Default for LogWriter<W> {
    fn default() -> Self {
        Self {
            log_file: None,
            prefixed_writer: None,
        }
    }
}

impl<W: Write> LogWriter<W> {
    pub fn with_log_file(&mut self, log_file_path: &AbsoluteSystemPath) -> Result<(), Error> {
        log_file_path.ensure_dir().map_err(|err| {
            warn!("error creating log file directory: {:?}", err);
            Error::CannotWriteLogs(err)
        })?;

        let log_file = log_file_path.create().map_err(|err| {
            warn!("error creating log file: {:?}", err);
            Error::CannotWriteLogs(err)
        })?;

        self.log_file = Some(BufWriter::new(log_file));

        Ok(())
    }

    pub fn with_prefixed_writer(&mut self, prefixed_writer: PrefixedWriter<W>) {
        self.prefixed_writer = Some(prefixed_writer);
    }
}

impl<W: Write> Write for LogWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match (&mut self.log_file, &mut self.prefixed_writer) {
            (Some(log_file), Some(prefixed_writer)) => {
                let _ = prefixed_writer.write(buf)?;
                log_file.write(buf)
            }
            (Some(log_file), None) => log_file.write(buf),
            (None, Some(prefixed_writer)) => prefixed_writer.write(buf),
            (None, None) => {
                // Should this be an error or even a panic?
                debug!("no log file or prefixed writer");
                Ok(0)
            }
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if let Some(log_file) = &mut self.log_file {
            log_file.flush()?;
        }
        if let Some(prefixed_writer) = &mut self.prefixed_writer {
            prefixed_writer.flush()?;
        }

        Ok(())
    }
}

pub fn replay_logs<W: Write>(
    output: &mut PrefixedUI<W>,
    log_file_name: &AbsoluteSystemPath,
) -> Result<(), Error> {
    debug!("start replaying logs");

    let log_file = File::open(log_file_name).map_err(|err| {
        warn!("error opening log file: {:?}", err);
        Error::CannotReadLogs(err)
    })?;

    let log_reader = BufReader::new(log_file);

    for line in log_reader.lines() {
        let line = line.map_err(Error::CannotReadLogs)?;
        output.output(line);
    }

    debug!("finish replaying logs");

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{fs, io::Write};

    use anyhow::Result;
    use tempfile::tempdir;
    use turbopath::AbsoluteSystemPathBuf;

    use crate::{
        logs::{replay_logs, PrefixedUI},
        LogWriter, PrefixedWriter, BOLD, CYAN, UI,
    };

    #[test]
    fn test_log_writer() -> Result<()> {
        let dir = tempdir()?;
        let log_file_path = AbsoluteSystemPathBuf::try_from(dir.path().join("test.txt"))?;
        let mut prefixed_writer_output = Vec::new();
        let mut log_writer = LogWriter::default();
        let ui = UI::new(false);

        log_writer.with_log_file(&log_file_path)?;
        log_writer.with_prefixed_writer(PrefixedWriter::new(
            ui,
            CYAN.apply_to(">".to_string()),
            &mut prefixed_writer_output,
        ));

        writeln!(log_writer, "one fish")?;
        writeln!(log_writer, "two fish")?;
        writeln!(log_writer, "red fish")?;
        writeln!(log_writer, "blue fish")?;

        log_writer.flush()?;

        assert_eq!(
            String::from_utf8(prefixed_writer_output)?,
            "\u{1b}[36m>\u{1b}[0mone fish\n\u{1b}[36m>\u{1b}[0mtwo fish\n\u{1b}[36m>\u{1b}[0mred \
             fish\n\u{1b}[36m>\u{1b}[0mblue fish\n"
        );

        let log_file_contents = log_file_path.read_to_string()?;

        assert_eq!(
            log_file_contents,
            "one fish\ntwo fish\nred fish\nblue fish\n"
        );

        Ok(())
    }

    #[test]
    fn test_replay_logs() -> Result<()> {
        let ui = UI::new(false);
        let mut output = Vec::new();
        let mut prefixed_ui = PrefixedUI::new(
            ui,
            CYAN.apply_to(">".to_string()),
            BOLD.apply_to(">!".to_string()),
            &mut output,
        );
        let dir = tempdir()?;
        let log_file_path = AbsoluteSystemPathBuf::try_from(dir.path().join("test.txt"))?;
        fs::write(&log_file_path, "\none fish\ntwo fish\nred fish\nblue fish")?;
        replay_logs(&mut prefixed_ui, &log_file_path)?;

        assert_eq!(
            String::from_utf8(output)?,
            "\u{1b}[36m>\u{1b}[0m\n\u{1b}[36m>\u{1b}[0mone fish\n\u{1b}[36m>\u{1b}[0mtwo \
             fish\n\u{1b}[36m>\u{1b}[0mred fish\n\u{1b}[36m>\u{1b}[0mblue fish\n"
        );

        Ok(())
    }
}
