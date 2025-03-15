# rusty-psn

A simple tool to grab updates for PS3 & PS4 games, directly from Sony's servers
using their updates API. Available for both Linux and Windows, in both GUI and
CLI alternatives.

## Usage

Go to the
[latest release](https://github.com/RainbowCookie32/rusty-psn/releases/latest)
page and download the file you'll use from the Assets section:

- If you want to use the GUI version of rusty-psn, then download the
  `rusty-psn-egui-windows.zip`, `rusty-psn-egui-linux.zip` or
  `rusty-psn-egui-macos-universal.zip` file, depending on your OS.

- If you want to use the CLI version of rusty-psn, then download the
  `rusty-psn-cli-windows.zip`, `rusty-psn-cli-linux.zip` or
  `rusty-psn-cli-macos-universal.zip` file, depending on your OS.

After the selected file is downloaded, **extract it** and run the executable
file. For the Linux egui builds, you'll need to install some dependencies
(sourced from
[egui's README](https://github.com/emilk/egui/blob/0.26.2/README.md)):``.
For macOS egui builds, it's recommended to move the extracted bundle into the 
Applications folder.

- Ubuntu:

```sh
sudo apt-get install libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
libspeechd-dev libxkbcommon-dev libssl-dev
```

- Fedora:

```sh
dnf install clang clang-devel clang-tools-extra speech-dispatcher-devel \
libxkbcommon-devel pkg-config openssl-devel libxcb-devel
```

- Arch Linux (guesswork from looking at Ubuntu's package, my terminal history
  doesn't go that far):

```sh
sudo pacman -S libxcb libxkbcommon
```

## Docker

Use the supplied Dockerfile to run the rusty-psn CLI on Linux or macOS.
Build and run as follows:

```sh
docker build . -t rusty-psn
docker run --rm -v ${PWD}/pkgs:/rusty-psn/pkgs rusty-psn
```

---

## Screenshots

- GUI Build:

![GUI Screenshot](https://github.com/user-attachments/assets/31049d75-ffbb-4d27-9bfb-d33624bc83cb)

- CLI Build:

![CLI Screenshot](https://user-images.githubusercontent.com/16805474/155437829-d9af7847-c005-4c5b-b281-7cb728f32c4d.png)
