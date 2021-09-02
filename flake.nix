{
  description = "Tomat Discord bot";

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
        })
      ];

      pkgs = import nixpkgs {
        inherit system overlays;
      };
    in
    rec
    {
      packages.tomat = pkgs.rustPlatform.buildRustPackage {
        pname = "tomat";
        version = "0.1.0";

        src = ./.;

        cargoSha256 = "sha256-QoI3RRCLc348swpHXXkUkcK47AQBB7ZpBiuSX4OfG1k=";
      };
      defaultPackage = packages.tomat;

      apps.tomat = flake-utils.lib.mkApp {
        drv = packages.tomat;
      };
      defaultApp = apps.tomat;

      devShell = pkgs.mkShell {
        nativeBuildInputs = with pkgs; [
          rust-toolchain
          nixpkgs-fmt
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
