use crate::config;
use chrono::Local;
use log::Level;

fn colored_level(level: Level) -> &'static str {
    match level {
        Level::Error => "\x1b[31;1mERROR\x1b[0m",
        Level::Warn => "\x1b[33;1mWARNING\x1b[0m",
        Level::Info => "\x1b[32;1mINFO\x1b[0m",
        Level::Debug => "\x1b[36;1mDEBUG\x1b[0m",
        Level::Trace => "\x1b[35;1mTRACE\x1b[0m",
    }
}

pub fn init_logger() -> anyhow::Result<()> {
    let log_dir = config::get_config_dir()?.join("logs");
    std::fs::create_dir_all(&log_dir)?;
    let log_path = log_dir.join("rsigild.log");

    fern::Dispatch::new()
        .level(log::LevelFilter::Debug)
        .chain(
            fern::Dispatch::new()
                .format(|out, message, record| {
                    out.finish(format_args!(
                        "\x1b[38;20m{} - {} - {}\x1b[0m - {}",
                        Local::now().format("%Y-%m-%d %H:%M:%S"),
                        record.target(),
                        colored_level(record.level()),
                        message
                    ))
                })
                .chain(std::io::stdout()),
        )
        .chain(
            fern::Dispatch::new()
                .format(|out, message, record| {
                    out.finish(format_args!(
                        "{} - {} - {} - {}",
                        Local::now().format("%Y-%m-%d %H:%M:%S"),
                        record.target(),
                        record.level(),
                        message
                    ))
                })
                .chain(fern::log_file(log_path)?),
        )
        .apply()?;

    Ok(())
}
