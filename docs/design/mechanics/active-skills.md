# Active Skills

Active skills are skills that need to be manually triggered by the player and have an active visual effect and usually come with their own animations.
For example, a basic sword slash, or a spinning melee attack, or shooting guns, or a dash
All active skills can either be used independently of the weapon the player uses (like a dash, or putting a turret on the ground)
Or they are dependent only on the category of the weapon: melee or ranged, and since the player always has 1 melee and 1 ranged weapon equipped, using those skills will automatically use the correct weapon.

So for instance, if you have a 'spinning melee attack' equipped and you have a hammer in your melee weapon slot, you will start spinning with a hammer.
But if you use this attack while having a sword in your melee weapon slot you will start spinning with a sword.

This means there's no need to ever manually swap your weapons, and all skills you equip will always be available to you
There will be around 30 active skills, and you will be able to equip 4-5 of them at a time (I'll have to see what number feels the best in game)
in terms of acquiring the skills, I will initially make it so that every time you kill an enemy you have a small chance that a random skill will be unlocked. Later I might come up with some better way of progression, but this should work for now

Also, one important design goal is to minimize the interdependence between movement and attacks: for example you can perform all basic attacks in all states, when running, jumping and falling, and the attacks won't slow you down or impact your motion
the only instance where active skills will impact your motion is when those skills are movement related skills, like a dash
the overall goal here is to make the combat as seamless as possible, no need to think about swapping weapons or whether you're currently jumping or running, when you hit that attack button you will always get the desired effect
I'm also considering implementing a very basic auto-aim for ranged weapons, so if there are no enemies in the direction where you are running you will shoot backwards
but if there are enemies in both directions then you shoot in direction of motion.