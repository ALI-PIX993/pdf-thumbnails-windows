# Prototype validation

The original provider was tested against PDFCraft commit
`5d590604867fdac7ed71c555c40477ae3331a60a` with the included renderer patch.

- 169 renderer unit tests passed, including bounds, invalid inputs, and rotations.
- One native COM/GDI test passed: stream initialization, class factory, actual first-page bitmap pixels, error paths, and object lifetime.
- The stripped x64 GNU development DLL loaded and exposed the required COM exports.
- After the user enabled it, registry bindings, DLL presence, and COM activation were verified.
- Visual Explorer output and cache behavior remain unconfirmed.

Before a stable release, test Explorer on clean Windows systems, provider replacement and restoration, upgrades, uninstall, and representative real-world PDFs. Run formatting and Clippy; the original sandbox could not canonicalize directories and those checks did not complete. Full upstream workspace and WASM gates were not completed.

The Windows module contains the narrowly scoped unsafe code required for the COM ABI and GDI bitmap allocation. PDF parsing and rendering use the safe Rust API. This is an independent project; upstream acceptance of the bridge has not been requested or granted.
