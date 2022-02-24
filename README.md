# rusty-psn
A simple tool to grab updates for PS3 games, directly from Sony's servers using their updates API. Available for both Linux and Windows, in both GUI and CLI alternatives.

The Linux GUI build might need some extra dependencies installed to work. According to egui's README:

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

---

## Screenshots

- GUI Build:

![GUI Screenshot](https://user-images.githubusercontent.com/16805474/155437468-ee810763-412b-4e48-8ef7-03e5015a76c0.png)

- CLI Build:

![CLI Screenshot](https://user-images.githubusercontent.com/16805474/155437829-d9af7847-c005-4c5b-b281-7cb728f32c4d.png)
