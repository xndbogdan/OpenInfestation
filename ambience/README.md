# The port's ambience sounds

The sound the port adds under Vulkan ([the plan](../docs/rt-plans/atmosphere.md)) plays from one file,
`Remastered.pak`, beside the game's data: the player's game folder, and
`retail/` for development. Without it the remastered sounds are off
and retail's sound plays as always; the Sound menus' Ambience row reads
Unavailable.

`manifest.json` says what goes in: each sound's source under a named root
(your own sound folders, named by `INF_SOUNDS_PACKS4` and
`INF_SOUNDS_SPLICE` or `--root`), its licence and its processing (trim,
filters, loudness, mono or stereo), and the credits every licence asks for.
The builder makes the pak from it:

```sh
python -I scripts/build-remastered-pak.py
python -I scripts/build-remastered-pak.py --root packs4=../sounds --out ../Infestation/Remastered.pak
```

It needs ffmpeg. Each sound becomes Ogg Vorbis at 44.1 kHz, the mixer's
rate, held under -2 dBFS by a limiter; `credits.txt` goes in beside them.
The sources are only read. `beds/` holds the long loops, `sweet/` the
one-shots, `pulse/` the Brain's heartbeats and `place/` the mono loops
heard from where things are (shores, machines), `crystal/` the crystals'
energy-shield rings (Splice's; the `splice` root is the Splice app's
`Samples/packs` folder); `modern_tuning.json`'s `atmosphere` says which
world plays which.

Rules:

- The sounds and the pak never enter git (`.gitignore`); only the
  manifest does.
- Each entry names its licence. A licence that does not allow shipping in
  a game (the Lucid pack's, until Lucid Samples answers) keeps its sounds
  to trials.
- No animal calls: in the distance one reads as an enemy.
