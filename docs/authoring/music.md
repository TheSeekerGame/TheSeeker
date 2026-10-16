# Music and Tuning

The Seeker's music, sound, animation, and gameplay share one musical foundation: a **fundamental of 48 Hz**. Use this page when composing music, designing sounds, or tuning anything pitched for the game or for any other work set in its universe.

## The tuning rule

A pitch belongs to the Seeker tuning when its frequency is the fundamental multiplied by a ratio of the form

```text
frequency = 48 Hz × X / 2^N        (X and N are whole numbers)
```

The denominator must be a power of two: 1, 2, 4, 8, 16, and so on.

| Ratio | Frequency | Valid? |
| --- | ---: | --- |
| 9/8 | 54 Hz | Yes: the denominator is 8 = 2³ |
| 19/16 | 57 Hz | Yes: 16 = 2⁴ |
| 13/8 | 78 Hz | Yes |
| 3/2 | 72 Hz | Yes |
| 7/4 | 84 Hz | Yes |
| 4/3 | 64 Hz | **No**: 3 is not a power of two |
| 6/5 | 57.6 Hz | **No**: 5 is not a power of two |

In other words, every note is a member of the **harmonic series** of 48 Hz or of one of its lower octaves (24, 12, 6 Hz, and so on), folded into whichever octave you need. Doubling or halving any valid frequency gives another valid frequency, so the rule works in every register.

This is not twelve-tone equal temperament, and it is not a fixed scale: any whole-number X is allowed, so the set of available pitches is open-ended. Higher values of X reach more unusual intervals (11/8, 13/8, 19/16, 21/16), while low ones give familiar consonances (3/2, 5/4, 7/4).

## The same rule shapes time

The game's timing follows the same rule, so rhythm, animation, and pitch all share one family:

| Quantity | Rate | As a ratio of 48 Hz |
| --- | ---: | ---: |
| Musical fundamental | 48 Hz | 1 |
| Simulation tick | 96 Hz | 2 |
| Standard animation | 12 Hz | 1/4 |
| Beat (90 BPM) | 1.5 Hz | 1/32 |

Because everything sits on one family of frequencies, sounds triggered by animation and gameplay land on the musical grid. [Timing, Audio, and Animation](../architecture/timing-and-audio.md) explains how the engine keeps them in sync.

## The Seeker tuning file

[SeekerTuning48hz.tun](SeekerTuning48hz.tun) is a ready-made keyboard mapping for synthesizers that read AnaMark `.tun` files (it also contains a VAZ Plus section). It picks six pitches per octave from the rule:

| Ratio | Frequency at 48 Hz | Cents above 48 Hz |
| --- | ---: | ---: |
| 1/1 | 48 Hz | 0 |
| 9/8 | 54 Hz | 203.9 |
| 19/16 | 57 Hz | 297.5 |
| 21/16 | 63 Hz | 470.8 |
| 3/2 | 72 Hz | 702.0 |
| 7/4 | 84 Hz | 968.8 |

MIDI key 12 plays 48 Hz and key 0 plays 24 Hz. Each block of twelve keys repeats the six pitches one octave higher. Several neighboring keys share a pitch, and the second key of each block jumps up an octave, so the layout is meant to be played rather than read as a chromatic scale. You can open and edit it in [Scale Workshop](https://sevish.com/scaleworkshop/).

The file is one selection from the rule, not its limit. Build other selections the same way: choose ratios with power-of-two denominators and anchor them to 48 Hz or one of its octaves.

## Composing in the Seeker style

- **Stay in the family.** Keep pitched material on valid ratios. Unpitched sounds, noise, and silence are free.
- **Let mathematics shape the music.** Phrase lengths, rhythms, and melodies can follow sequences such as primes or the Fibonacci numbers. Listeners shouldn't need to notice; the music should simply feel right.
- **Write for the beat.** At 90 BPM, a beat lasts eight animation frames and 64 simulation ticks, so musical events and on-screen motion line up naturally.

See [Lore](../design/lore.md#mathematics-as-experience) for the ideas behind this approach and [Assets](assets.md) for audio file formats.
