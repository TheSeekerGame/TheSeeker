# Passive Skills

Passive skills are skills that are always active and have a passive effect on you, and that mostly means modifying certain stats based on specific conditions
For example: 'deal double damage when you're at very low health', or 'deal more damage when attacking from behind', or 'deal more damage when there are no enemies in close proximity to you', or 'critical hits reduce your active skills cooldowns', etc
Each one of those passive skills is meant to incentivize a specific kind of playstyle
For example, the passive that makes you deal more damage when no enemies are nearby incentivizes ranged builds that involve staying away from enemies, so ranged attacks that stun or push back or slow down your enemies will work well with this.
Since those passives are mostly just modifying variables based on If statements, it should be very easy to implement a lot of those
I'm thinking having between 30 and 50 should be relatively easy for a start
The player will be able to have around 5 of those passives equipped at a time
The power of this approach is that if we assume 50 passives where you can pick 5 at a time, that's already 2,118,760 possible combinations, just with those passives, without even including the active skills or the weapon choices.
An important design goal here is to make those passives as compatible with eachother as possible.