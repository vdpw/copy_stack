use crate::store::HistoryJsonlConfig;
use std::ffi::OsString;
use std::path::PathBuf;

const DEFAULT_JSONL_MAX_DATA_BYTES: usize = 4096;
const JSONL_PATH_FLAG: &str = "--copy-stack-history-jsonl";
const JSONL_MAX_DATA_BYTES_FLAG: &str = "--copy-stack-history-jsonl-max-data-bytes";

#[derive(Clone, Debug, Default)]
pub struct StartupOptions {
    pub history_jsonl: Option<HistoryJsonlConfig>,
}

impl StartupOptions {
    pub fn from_env_args() -> Result<Self, String> {
        Self::from_args(std::env::args_os().skip(1))
    }

    fn from_args<I>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = OsString>,
    {
        let mut history_jsonl_path = None;
        let mut max_data_bytes = DEFAULT_JSONL_MAX_DATA_BYTES;
        let mut args = args.into_iter().peekable();

        while let Some(arg) = args.next() {
            let Some(arg) = arg.to_str() else {
                continue;
            };

            if let Some(path) = arg.strip_prefix(&format!("{}=", JSONL_PATH_FLAG)) {
                history_jsonl_path = Some(PathBuf::from(path));
            } else if arg == JSONL_PATH_FLAG {
                let path = args
                    .next()
                    .ok_or_else(|| format!("{} requires a file path", JSONL_PATH_FLAG))?;
                history_jsonl_path = Some(PathBuf::from(path));
            } else if let Some(value) = arg.strip_prefix(&format!("{}=", JSONL_MAX_DATA_BYTES_FLAG))
            {
                max_data_bytes = parse_max_data_bytes(value)?;
            } else if arg == JSONL_MAX_DATA_BYTES_FLAG {
                let value = args.next().ok_or_else(|| {
                    format!("{} requires a byte count", JSONL_MAX_DATA_BYTES_FLAG)
                })?;
                let value = value.to_string_lossy();
                max_data_bytes = parse_max_data_bytes(&value)?;
            }
        }

        Ok(Self {
            history_jsonl: history_jsonl_path.map(|path| HistoryJsonlConfig {
                path,
                max_data_bytes,
            }),
        })
    }
}

fn parse_max_data_bytes(value: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .map_err(|error| format!("invalid JSONL max data byte count: {}", error))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os_args(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn startup_options_parse_history_jsonl_flags() {
        let options = StartupOptions::from_args(os_args(&[
            "--ignored-tauri-flag",
            "--copy-stack-history-jsonl",
            "/tmp/copy-stack.jsonl",
            "--copy-stack-history-jsonl-max-data-bytes=32",
        ]))
        .expect("options should parse");
        let config = options.history_jsonl.expect("JSONL should be enabled");

        assert_eq!(config.path, PathBuf::from("/tmp/copy-stack.jsonl"));
        assert_eq!(config.max_data_bytes, 32);
    }

    #[test]
    fn startup_options_support_equals_path_flag() {
        let options =
            StartupOptions::from_args(os_args(&["--copy-stack-history-jsonl=/tmp/out.jsonl"]))
                .expect("options should parse");
        let config = options.history_jsonl.expect("JSONL should be enabled");

        assert_eq!(config.path, PathBuf::from("/tmp/out.jsonl"));
        assert_eq!(config.max_data_bytes, DEFAULT_JSONL_MAX_DATA_BYTES);
    }
}
