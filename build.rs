use std::path::Path;

fn main() {
	// Re-run resource setup whenever the optional application icon changes.
	println!("cargo:rerun-if-changed=assets/icon.ico");

	// The Windows resource compiler is only used for Windows executables.
	#[cfg(windows)]
	compile_windows_resources();
}


#[cfg(windows)]
fn compile_windows_resources() {
	let mut resources = winresource::WindowsResource::new();
	// Add the application icon when the repository includes it.
	if Path::new("assets/icon.ico").exists() {
		resources.set_icon("assets/icon.ico");
	} else {
		println!("cargo:warning=assets/icon.ico is missing; building without an application icon");
	}
	// Set the names displayed in Windows file properties and Task Manager.
	resources.set("FileDescription", "Hover Over");
	resources.set("ProductName", "Hover Over");
	// Compile the Windows version information and icon into the executable.
	resources.compile().expect("Windows resources failed to compile; check that the Windows SDK resource compiler rc.exe is installed");
}
