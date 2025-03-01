{ pkgs, lib, config, inputs, ... }:
{
  env.GREET = "devenv";
  packages = with pkgs; [
    cmake
    pkg-config
    pkgsStatic.openssl.dev
    pkgsStatic.openssl
    bashInteractive
  ];

  languages.rust = {
    enable = true;
    channel = "stable";
  };

  enterShell = ''
  '';
}
