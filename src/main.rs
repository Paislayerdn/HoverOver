mod app;
mod geometry;
mod image_file;
mod model;
mod renderer;
mod windows_overlay;

fn main() {
	if let Err(error) = windows_overlay::run() {
		eprintln!("HoverOver could not start: {error}");
		std::process::exit(1);
	}
}
