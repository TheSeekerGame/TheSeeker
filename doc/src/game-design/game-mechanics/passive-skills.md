# Passive Skills

Passive skills are skills that have an ongoing effect and don't need manual activation.

They are mostly focused on modifying certain stats based on specific conditions.

They usually have very little or no visual effect, they are just conditional stat modifiers.

The player can choose up to 4 passives to be equipped at once (although we might add a passives that unlock additional passive slots)

Passives work as 'functions', each accepting as input the player stats spitted out by the previous passives, etc

This means that ordering of passives matters, and players can chain them up as they choose

Some passives might be strongly affected by ordering, while others not at all

Some examples of Passive skills could be:
- 'get higher crit chance when you're at very low health'
- 'deal more damage when attacking from behind'
- 'attack faster when there are no enemies in close proximity to you'
- 'critical hits reduce your active skills cooldowns'
- etc

Passives can also affect the effects of other passives, or even amounts of passives that the player can equip.

For example there could be a passive like:
- Double the effects of all your other passives, but max passive slots get reduced to 3.

Each one of those passive skills is meant to incentivize a specific kind of playstyle.

For example, the passive that makes you attack faster when no enemies are nearby incentivizes ranged builds that involve staying away from enemies, so ranged attacks that stun or push back or slow down your enemies will work well with this. Then the increased attack speed might also be a good combination with effects that proc 'per hit', etc.

Since those passives are mostly just modifying variables based on If statements, it should be relatively easy to implement a lot of those for the player to choose from, leading to astronomical amounts of possible permutations.

An important design goal is to make the passives as compatible with eachother as possible.