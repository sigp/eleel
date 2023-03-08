use slog::Logger;
use tracing_slog::TracingSlogDrain;

pub fn new_logger() -> Logger {
    Logger::root(TracingSlogDrain, slog::o!())
}
