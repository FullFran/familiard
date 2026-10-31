# Architecture

Hexagonal. Dependencies point inward, and the domain depends on nothing, not even a
library. To add something, you normally write one new crate and touch one line in `cli`.

```
cli (composition root: picks the adapters)
 ├─ pack             PetSource: reads pet folders
 ├─ stage-wayland    Stage: floating surface on Wayland
 ├─ stage-windows    Stage: layered window on Windows
 ├─ stage-terminal   Stage: half blocks in a terminal
 ├─ caption          Captioner: writes a caption with an embedded font
 ├─ gallery          the pets the user keeps, and the default one
 ├─ agents           ActivityFeed and Roster: every open pet hears hooks over loopback
 └─ application      the ports, and Player, which runs the pet for a stage
     └─ domain       Pet, Companion, Activity, Sprite
```

| Concept | Meaning |
| --- | --- |
| `Pet` | A character: a clip per mood |
| `Companion` | A pet alive: its current mood and frame |
| `Activity` | Turns what the sessions it cares about report into one mood, and says when the one it follows has ended |
| `Player` | What a stage talks to: "what do I draw, and when do I ask again?" |

## Where things go

| I want to add | Implement | Example idea |
| --- | --- | --- |
| A place to show the pet | `Stage` | Windows, macOS, an editor panel, a Pi extension |
| A way to hear about activity | `ActivityFeed` | CI status, a calendar |
| A pet format | `PetSource` | A folder of GIFs |
| A new behaviour or mood | `domain` | Falling asleep |
| Another way of writing captions | `Captioner` | A font of the user's choosing |

## Adding a stage

A stage owns its event loop, because most toolkits insist on running it. It asks the
`Player` what to do:

```rust
impl Stage for MyStage {
    fn perform(self: Box<Self>, mut player: Player) -> Result<(), Failure> {
        let (width, height) = player.bounds(); // the box that fits every frame and caption
        player.paint(&mut canvas); // premultiplied BGRA: the sprite and any caption
        show(&canvas);
        while !player.leaving() {
            let wait = player.next_frame_in();
            sleep(wait);
            if player.tick(wait).is_some() {
                player.paint(&mut canvas);
                show(&canvas);
            }
        }
        Ok(())
    }
}
```

`crates/stage-terminal` is the smallest example, and it prints `Player::caption_text` instead of
pixels, using `Player::pet_bounds` for the frames. Draw frames bottom-centered in the box, so
that a pet of varying width does not jump around (`Player::paint` does this for desktops, and
returns where the sprite landed). Let clicks through transparent areas: Windows does it by
itself, and on Wayland `Sprite::solid_spans` gives the region to use.

When `Player::leaving` is true the pet's session has ended: end the loop and return `Ok`.

Captions cost nothing while running. `Player::captioned` draws every caption the pet can show
once, reserves the box for the widest, and lets go of the `Captioner`, because an open font takes
several megabytes.

Stages that exist only on one system use `#[cfg(target_os = "...")]`, as `stage-wayland` does.

## Rules

- Nothing in `domain` or `application` may know about a display, a socket or a file.
- Time is passed in (`advance`, `tick`), so behaviour is tested without waiting.
- Each adapter has its own tests; the hook is tested on the real binary, because other
  people's agents depend on it.
