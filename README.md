# PDF thumbnails for Windows

Show the first page of a PDF as its thumbnail in Windows File Explorer. Works regardless of your default PDF reader: PDFCraft, Adobe Acrobat, Edge, or another application. Your default reader stays unchanged.

This is an independent, unofficial project using PDFCraft's open-source renderer. It changes Explorer thumbnails; it does not add previews inside other applications. Windows x64 is currently supported.

## Install

Download the Windows x64 ZIP from this repository's Releases page. Extract the whole folder to a permanent location and double-click **Enable thumbnails.cmd**. No administrator access is needed. Sign out and back in, then choose **Large icons** or **Extra large icons** in File Explorer.

In Explorer's **Options > View**, clear **Always show icons, never thumbnails**. Windows may cache old icons; use Windows Disk Cleanup's thumbnail-cache option if needed. Keep the extracted folder in place while enabled.

To undo, run **Disable thumbnails.cmd** before moving or deleting the folder, then sign out and back in. The script preserves and restores any previous per-user provider. It refuses to replace another registered provider unless you explicitly pass `-ReplaceExisting`.

## Build from source

Install Git, Rust 1.90 or newer, and the Windows C++ build tools for the standard Rust MSVC toolchain. Run from this repository:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File build.ps1
```

The build script fetches the pinned PDFCraft commit, applies the included safe renderer patch, copies this repository's COM provider into that workspace, runs renderer and native integration tests, and builds the DLL. It writes an installable folder under `dist/windows-x64`. The PDFCraft GUI is not built or packaged. The upstream checkout and Cargo dependency cache are build inputs, not part of this repository.

The provider source is in `lib.rs` and `windows.rs`. `renderer-thumbnail.patch` adds a bounded first-page rendering API with rotation and invalid-input tests. The DLL and CLSID retain their original names for compatibility with the prototype already installed by early users.

## Status and limits

This is a prototype. The initial x64 GNU development package passed 169 renderer tests and one native COM/GDI integration test checking real bitmap pixels and object lifetime. Packaged DLL loading, Windows per-user registration, and COM activation were verified. Actual Explorer visual output, cache refresh, upgrade, and removal still need broader testing. The included binary is unsigned.

Password-required PDFs, invalid documents, and files over 64 MiB retain an icon. Online-only cloud files may need downloading first. Thumbnails are limited to 1024 pixels per side. Complex documents can be slow. The provider reads files without modifying them and does not execute PDF JavaScript. Windows' isolated thumbnail host remains enabled.

ARM64 and 32-bit builds are not supported yet. The initial environment could not complete Rustfmt or Clippy because of directory-access errors; these checks must run successfully before a stable release. See `VALIDATION.md`.

## Credits and license

MIT OR Apache-2.0. PDF rendering comes from [PDFCraft](https://github.com/storytold/pdfcraft), pinned to commit `5d590604867fdac7ed71c555c40477ae3331a60a`, copyright 2026 ArtCraft Team and PDFCraft contributors. This project is not an official PDFCraft or ArtCraft release and includes no ArtCraft logos.

Dependency code, fonts, color profiles, and character-mapping data retain their own licenses. The downloadable package includes third-party notices. `third-party-licenses.zip` retains the license texts for the pinned prototype dependency set. See `NOTICE` and the pinned upstream attribution files.
