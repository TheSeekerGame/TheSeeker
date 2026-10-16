# Passive Skills

Passives are always on. Each one changes your stats or the rules of combat under some condition, and each one nudges you toward a particular way of playing. You can equip four at a time.

## How passives work today

- **Crafted at capsules.** Passives come from capsule recipes, so the materials you collect decide which ones you can make. Seven are craftable so far; the rest are implemented and still being worked into progression.
- **Rolled values.** Many passives roll a strength within a range when you get them. Upgrading improves the roll, and its description always shows the current value.
- **Collected, then equipped.** Unlocked passives wait in your inventory until you equip them, so you can swap builds back at a capsule.

The passives implemented so far, grouped by the playstyle they encourage:

| Playstyle | Examples |
| --- | --- |
| Risk and low health | Frenzied Attack, Serpent Ring, Vitality Overclock, Death Shroud |
| Critical hits | Flaming Heart, Obsidian Necklace, Critical Regeneration |
| Position and movement | Sharpshooter, Heavy Boots, Deadly Feather, Rabbit's Foot |
| Stealth and backstabs | Ice Dagger, Shadow Cloak |
| Crowds and fresh targets | Glowing Shard, Pack Killer, Prime Sygil |
| Elemental conversion | Fire Crystal or Ice Crystal (mutually exclusive) |
| Extra effects | Prism splits projectiles, Kinetic Orb adds orbiting orbs, Pulse Echo sends out melee pulses |
| Skill rhythm | Permutator rewards alternating skills; Pulse Drive turns Dash Strike crits into instant-skill triggers; Battery Pack adds a fifth skill slot |
| Survival | Bloodstone heals from experience orbs; Blood Shards turns overhealing into homing shards; Protective Spirit caps the damage of a single hit |

## Design principles

- **Incentivize playstyles.** A passive that rewards distance from enemies pushes you toward ranged builds and toward skills that slow or push enemies away. Each passive should suggest a way to play, not just a bigger number.
- **Maximize compatibility.** Passives should combine with each other and with active skills as freely as possible. Mutually exclusive pairs, like the two elemental crystals, are rare and deliberate.
- **Combinatorial depth.** Picking four from even a modest pool gives thousands of loadouts, before counting skills and weapons. The aim is that a large share of them feel genuinely different.

## Where it's heading

The pool keeps growing, and the direction is for passives to tie into the [permutation](README.md#permutations-where-its-heading) loop: materials that remember how a creature was defeated unlock different recipes, and so different passives. The equipment a Seeker carries ends up recording their relationship with the planet.
