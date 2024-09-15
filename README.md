# rusty-psn
A simple tool to grab updates for PS3 & PS4 games, directly from Sony's servers using their updates API. Available for both Linux and Windows, in both GUI and CLI alternatives.

## Usage
Go to the [latest release](https://github.com/RainbowCookie32/rusty-psn/releases/latest) page and download the file you'll use from the Assets section:
- If you want to use the GUI version of rusty-psn, then download the `rusty-psn-egui-windows.zip` or `rusty-psn-egui-linux.zip` file, depending on your OS.
- If you want to use the CLI version of rusty-psn, then download the `rusty-psn-cli-windows.zip` or `rusty-psn-cli-linux.zip` file, depending on your OS.
- If you are using macOS, you can use the Dockerfile below to run the CLI build of rusty-psn. While the egui build might be able to compile and run natively on macOS, I don't have the means to test it so it's an unsupported configuration. You are on your own.

After the selected file is downloaded, **extract it** and run the executable file. For the Linux egui builds, you'll need to install some dependencies (sourced from [egui's README](https://github.com/emilk/egui/blob/0.26.2/README.md)):

- Ubuntu:
```
sudo apt-get install libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libspeechd-dev libxkbcommon-dev libssl-dev
```

- Fedora:
```
dnf install clang clang-devel clang-tools-extra speech-dispatcher-devel libxkbcommon-devel pkg-config openssl-devel libxcb-devel
```
- Arch Linux (guesswork from looking at Ubuntu's package, my terminal history doesn't go that far):
```
sudo pacman -S libxcb libxkbcommon
```

## Docker

Use the supplied Dockerfile to run the rusty-psn CLI on Linux or macOS.
Build and run as follows:

```
docker build . -t rusty-psn
docker run --rm -v ${PWD}/pkgs:/rusty-psn/pkgs rusty-psn
```
---

## Screenshots

- GUI Build:

![GUI Screenshot](https://user-images.githubusercontent.com/16805474/155437468-ee810763-412b-4e48-8ef7-03e5015a76c0.png)

- CLI Build:

![CLI Screenshot](https://user-images.githubusercontent.com/16805474/155437829-d9af7847-c005-4c5b-b281-7cb728f32c4d.png)
