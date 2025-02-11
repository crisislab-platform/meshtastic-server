{
  description = "CRISiSLab Meshtastic server";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
		flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }: flake-utils.lib.eachDefaultSystem (system: let
		pkgs = import nixpkgs { inherit system; };
  in {
		devShells = {
			mqtt = pkgs.mkShell {
				buildInputs = with pkgs; [
					mosquitto
					openssl
				];
			};

			api = pkgs.mkShell {
				buildInputs = with pkgs; [
					protobuf
				];
			};
		};
	});
}
