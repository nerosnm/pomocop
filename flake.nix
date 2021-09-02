{
  description = "Tomat Discord bot";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    naersk.url = "github:nmattia/naersk";
  };

  outputs =
    { self
    , nixpkgs
    , flake-utils
    , rust-overlay
    , naersk
    } @ inputs:
    flake-utils.lib.eachDefaultSystem (system:
    let
      overlays = [ (import rust-overlay) ];

      pkgs = import nixpkgs {
        inherit system overlays;
      };

      rust-toolchain =
        (pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain).override {
          extensions = [ "rust-src" ];
        };

      # Override the version used in naersk
      naersk-lib = naersk.lib."${system}".override {
        rustc = rust-toolchain;
      };

      format-pkgs = with pkgs; [
        nixpkgs-fmt
      ];
    in
    rec
    {
      packages.tomat = naersk-lib.buildPackage {
        pname = "tomat";
        root = ./.;
        nativeBuildInputs = with pkgs; [
        ];
      };
      defaultPackage = packages.tomat;

      apps.tomat = flake-utils.lib.mkApp {
        drv = packages.tomat;
      };
      defaultApp = apps.tomat;

      devShell = pkgs.mkShell {
        nativeBuildInputs = with pkgs; [
          rust-toolchain
        ] ++ format-pkgs;
      };

      checks = {
        format = pkgs.runCommand
          "check-nix-format"
          { buildInputs = format-pkgs; }
          ''
            ${pkgs.nixpkgs-fmt}/bin/nixpkgs-fmt --check ${./.}
            touch $out
          '';
      };
    });
}
