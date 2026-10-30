# Optional retail test data

Copy a private Infestation installation into this folder. Tests read
`Infest.exe`, `PreLoad.dat` and the complete `Overlay/` directory (1,932
files); some also read `Infest.sav`, `Title.avi`, `cdaudio/`, the transcoded
`video/` and the demo's data in `infestation_demo/`. Everything here except
this setup note is Git-ignored.

`cargo run` from the repository root also falls back to this folder when no
`--data-root` is given. A directory junction or symlink to an installation
works too; tests only read it.

Do not add generated test output here.
