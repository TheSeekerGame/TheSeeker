# Art Direction

- Low-res pixel art (6x6 tile size, 12px player height)
- Low saturation
- No outlines
- Focus on good readability and color harmony
- Player should ideally be the only yellow'y asset in game to make it very visible
- Seeker technology is dark, minimalist sci-fi that fuses the technical with the organic: mostly black, with plenty of black organic detail and a pale core of light. The tether node is the example, a metal stake wrapped by a parasitic vine that roots into the rock
- All assets drawn fully lit and then darkness added as a post processing shader
- Multi-layered and parallaxed background and foregrounds
- Focus on auto-tiling, to save time designing levels
- Backgrounds and foreground mostly simple silhouettes so attention is on the main platforms
- Procedural fogs and blur layers to build atmosphere and make backgrounds grab even less attention
- Atmospheric lighting and bloom™️ 
- I would love to have lighting with normal maps at some point but it might be too much work
- All animations play at 12fps
- Shaders for environment so it reacts to player (like vegetation moves when you run through it, or water splashes etc)
- Procedural 'dust' trail behind player when jumping (like in hollow knight)

## Palette

These colors are sampled exactly from the game's art, which so far covers the icy surface, the ice caves, and the tundra below them. Each scene uses a coherent subset rather than every swatch. The planet grows warmer, stranger, and more exotic with depth, so deeper regions extend these ramps in that direction with the same restraint: calm fields, clear value structure, and a few accents that lead the eye.

| Family | Colors | Use |
| --- | --- | --- |
| Seeker gold and deepest shadow | `#B9A87B` `#6B6258` `#2D2E33` `#0F0E16` `#FFFFFF` | The player's muted gold, warm shadow, dark structure, and the visor's tiny highlight |
| Ice-cave stone | `#181C20` `#20242A` `#353E50` `#4F5D73` `#667389` `#94A6B1` | Charcoal through slate to exposed cool edges |
| Dark creature bodies | `#181C20` `#262B32` `#313C43` `#3F4D5B` `#4F5D73` `#C2C9C9` | Compact dark bodies with sparse pale details |
| Pale shells | `#353B44` `#465B7D` `#627594` `#7B92A9` `#97ACB0` `#C2C9C9` | Blue-gray carapace and anatomy |
| Minerals and snow | `#50607C` `#758399` `#94A6B1` `#C4CED4` `#EDF0F1` | Crystalline volumes and drifts |
| Cold luminescence | `#698F9B` `#7BACBC` `#91CBDE` `#D6EDF5` | Sparse local light, never a cyan wash |
| Tundra stone | `#21252C` `#2A2F32` `#353E50` `#3E545B` `#4F7368` `#557C70` `#8CA893` | Rock gradually admitting green and teal |
| Moss, bark, and earth | `#1C1D22` `#353B44` `#49573D` `#61554C` `#796E58` `#6A7972` `#779051` `#8CA893` `#C2C9C9` | Organic bodies and ground |
| Living accents | `#696E49` `#8C9245` `#AD2E41` `#AF634B` | Olive growth, berries, and small warm organic details |
| Atmospheric distance | `#5D666D` `#6C7983` `#83909C` `#8D9FAA` `#A1B2BB` `#B0C3CD` `#CFDDE4` `#D4E6EC` `#F5FAFD` | Increasingly pale, quiet distant planes |
| Neutral figure | `#010202` `#282E3C` `#5E6970` `#7E8991` `#F6FAFD` | A near-black face in a quiet gray body |
| A character who stands apart | `#000000` `#172808` `#654170` `#4C5C8C` `#5B8399` | Restrained blue-violet for an individual |
| Void | `#151515` | A silhouette with almost no interior detail |

The player's gold is a priority accent, not a rule that nothing else may be warm: plants and props already include warm colors. Keep the player readable through amount, placement, silhouette, and value wherever something warm is near.

The interface has its own palette, defined in `game/src/ui/pixel_panel.rs`: panel `#04060A` at 92% opacity, item well `#12151D`, empty well `#0B0D13`, line `#29303E`, text `#F0F2E8`, muted text `#8A919E`, disabled text `#454A4F`, positive feedback `#ABE3D9`, and negative feedback `#E84D3D`.