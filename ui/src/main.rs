mod app;
mod appearance;
mod catalog;
mod device;
mod draw;
fn main() {
    if let Err(error) = app::run() {
        eprintln!("GR-1 UI: {error}");
        std::process::exit(1);
    }
}
