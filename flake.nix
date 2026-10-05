# SPDX-FileCopyrightText: 2026 Froststrap
#
# SPDX-License-Identifier: MPL-2.0

{
  description = "Flake for Mod Generator";

  inputs = {
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
    nixpkgs.url = "github:nixos/nixpkgs/26.05";
    treefmt-nix.url = "github:numtide/treefmt-nix";
    self.submodules = true;
  };

  outputs =
    {
      flake-utils,
      nixpkgs,
      ...
    }@inputs:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
        };
      in
      {
        devShells =
          let
            inherit (pkgs.callPackage ./nix/devshell-tools.nix { })
              mkComposedShell
              ;

            extraFrag = pkgs.callPackage ./nix/extra.nix { };
            rustFrag = (pkgs.callPackage ./nix/rustDevShell.nix { }) inputs;
          in
          mkComposedShell [ rustFrag ] { }
          // mkComposedShell [
            rustFrag
            extraFrag
          ] { nameOverride = "default"; };

        formatter = (pkgs.callPackage ./nix/formatter.nix { }) inputs;
      }
    );
}
