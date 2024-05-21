# Cooldowns + Energy

It's quite common for games to use a combination of cooldowns and energy as a way to prevent the player from being able to use all abilities at all times.
However, I find that managing these 2 'resources' at the same time can be confusing and distracting for the player.
That's why in The Seeker the two are 'combined' into essentially the same thing.

Overall, active skills can be divided into '1 shots' and 'continuous'.


The '1 shots' are things like a basic sword slash, that just gets activated, does its thing, and then stops on its own.
The '1 shots' are the skills that will have regular 'cooldowns'.

The 'continuous' skills are for example a whirling attack where you continue to whirl around for as long as you keep pressing the button.
These skills will utilize 'energy' - when using the skill you keep using up energy and once no energy is left you can't use the skill any more until it recharges.

However, as stated before the 'cooldowns' and 'energy' are combined into kind of the same thing in this game.
The way this works is: energy is just a cooldown that works in reverse.

So when using a '1 shot' skill the cooldown goes to 100%, and then over time goes down. Once the cooldown goes back to 0 you can use the '1 shot' skill again.

But when using a 'continuous' skill, the cooldown starts from 0, and starts filling up as you keep using the continuous skill. Then once this 'cooldown' reaches 100% you can't use the skill anymore. Once you're not using the skill the cooldown starts decreasing again.
But here you don't need to wait for the cooldown to drop to 0 again, you can wait for example until it goes down to 90% and then use the skill again for a little bit until it goes to 100% again, etc.

In terms of UI, the skills will all have square shapes, then the regular cooldown should be displayed as a dark tint that is displayed on the entire ability and goes down over time (from top to bottom).
The energy should be displayed as a dark tint that starts from the top and goes downwards filling up more space the longer you keep using the skill.

