{
  lib,
  rustPlatform,
  pkg-config,
  libxkbcommon,
  wayland,
  wayland-protocols,
}:

rustPlatform.buildRustPackage {
  pname = "screenshot-overlay";
  version = "0.1.0";

  src = lib.cleanSource ./.;
  cargoLock.lockFile = ./Cargo.lock;

  nativeBuildInputs = [ pkg-config ];
  buildInputs = [
    libxkbcommon
    wayland
    wayland-protocols
  ];

  # The two `overlay::bottombar` tests assert that a longer label makes a wider
  # card; both need fonts, and the build sandbox has none. The other 38 pass.
  doCheck = false;

  meta = {
    description = "Region-selection overlay for the Noctalia screenshot plugin";
    license = lib.licenses.gpl3Plus;
    mainProgram = "screenshot-overlay";
    platforms = lib.platforms.linux;
  };
}
