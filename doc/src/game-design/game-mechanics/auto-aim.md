# Auto Aim

The general idea for Auto Aim is to make player's life easier, while also minimizing the risk of this mechanic being annoying or interfering with the player's intentions.

Each 'Basic' attack, melee or ranged, can be pointed in multiple directions. 

Melee attacks can point:
- Forward
- Up (straight)
- Down (straight)

Ranged attacks can point:
- Forward
- Up (straight)
- Up (45 degrees)
- Down (Straight)
- Down (45 degrees)

^all these will have their individual animation, so basic melee attacks will have 3 animations for each weapon, and basic ranged attacks will have 5 animations for each weapon.

By default, all weapons point forward. However, if there are no targets (including destructible objects etc) in the forward direction, **and** there are targets in some of the other directions, the attack will automatically pick that direction.
This should also apply to attacking backwards through sprite flipping, so if you're running forward and shoot a bow, and there are no enemies in front but some enemies behind, the sprite will flip for the duration of the attack and shoot backwards (without affecting velocity).

The player can also point their weapon manually by pressing up/down keys while attacking (like in Hollow Knight).
In this case if there are enemies in both forward and up/down (or no enemies in any direction), and player is holding the up key, the attack will be upward.
Only if there are no targets up **and** there are targets for example forward will the weapon attack forward despite the up button being pressed.

Notably there's only 1 up and 1 down button, but ranged weapons have 2 up/down options (straight up/down and 45 degree up/down).
The 45 degree variants should be picked by the auto aim using the same logic as all the other directions. If there's some enemy at 45 degree angle and no other enemies in other direction, the 45 degree angle will be picked.
If you're holding say an up button and there's an enemy at 45 degree angle, another enemy directly forward, and no enemies straight upwards, then the 45 degree angle will be picked (because the up button is pressed and the 45 degree is the only up option available)
If you're holding an up button and there are targets both straight upwards and at 45 degree angle, a random direction will be picked.