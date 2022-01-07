{
  description = "Pomocop Discord bot";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs =
    { self
    , nixpkgs
    , flake-utils
    , rust-overlay
    } @ inputs:
    flake-utils.lib.eachDefaultSystem (system:
    let
      overlays = [
        (import rust-overlay)
        (final: prev: {
          rust-toolchain =
            (pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain).override {
              extensions = [ "rust-src" ];
            };
            rustfmt = pkgs.rust-bin.nightly.latest.rustfmt;
        })
      ];

      pkgs = import nixpkgs {
        inherit system overlays;
      };
    in
    rec
    {
      packages.pomocop = pkgs.rustPlatform.buildRustPackage {
        pname = "pomocop";
        version = "0.1.0";

        src = ./.;

        cargoSha256 = "sha256-KYTQWNLlxittM9eepslQzbT9OV8ubj+2cclqrq5rGcM=";

        buildInputs = with pkgs; [
          sqlite
        ];
      };
      defaultPackage = packages.pomocop;

      apps.pomocop = flake-utils.lib.mkApp {
        drv = packages.pomocop;
      };
      defaultApp = apps.pomocop;

      devShell = pkgs.mkShell {
        nativeBuildInputs = with pkgs; [
          rustfmt
          rust-toolchain
          nixpkgs-fmt
        ];
        buildInputs = with pkgs; [
          sqlite
        ];
      };

      checks = {
        format = pkgs.runCommand
          "check-nix-format"
          { buildInputs = with pkgs; [ nixpkgs-fmt ]; }
          ''
            ${pkgs.nixpkgs-fmt}/bin/nixpkgs-fmt --check ${./.}
            touch $out
          '';
      };
    });
}
