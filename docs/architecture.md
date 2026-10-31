# Architecture

Hexagonal. Dependencies point inward, and the domain depends on nothing, not even a
library. To add something, you normally write one new crate and touch one line in `cli`.

```
cli (composition root: picks the adapters)
 ├─ pack             PetSource: reads pet folders
 ├─ stage-wayland    Stage: floating surface on Wayland
 ├─ stage-terminal   Stage: half blocks in a terminal
 ├─ agents           ActivityFeed: hears hooks over a loopback socket
 └─ application      the ports, and Player, which runs the pet for a stage
     └─ domain       Pet, Companion, Activity, Sprite
```

| Concept | Meaning |
| --- | --- |
| `Pet` | A character: a clip per mood |
| `Companion` | A pet alive: its current mood and frame |
| `Activity` | Turns what every session reports into one mood |
| `Player` | What a stage talks to: "what do I draw, and when do I ask again?" |

## Where things go

| I want to add | Implement | Example idea |
| --- | --- | --- |
| A place to show the pet | `Stage` | Windows, macOS, an editor panel, a Pi extension |
| A way to hear about activity | `ActivityFeed` | CI status, a calendar |
| A pet format | `PetSource` | A folder of GIFs |
| A new behaviour or mood | `domain` | Falling asleep |

## Adding a stage

A stage owns its event loop, because most toolkits insist on running it. It asks the
`Player` what to do:

```rust
impl Stage for MyStage {
    fn perform(self: Box<Self>, mut player: Player) -> Result<(), Failure> {
        let (width, height) = player.bounds(); // the box that fits every frame
        draw(player.sprite());
        loop {
            let wait = player.next_frame_in();
            sleep(wait);
            if let Some(sprite) = player.tick(wait) {
                draw(sprite); // straight-alpha RGBA
            }
        }
    }
}
```

`crates/stage-terminal` is the smallest example. Draw frames bottom-centered in the box, so
that a pet of varying width does not jump around. On desktops, use `Sprite::solid_spans` to
let clicks through transparent areas.

Stages that exist only on one system use `#[cfg(target_os = "...")]`, as `stage-wayland` does.

## Rules

- Nothing in `domain` or `application` may know about a display, a socket or a file.
- Time is passed in (`advance`, `tick`), so behaviour is tested without waiting.
- Each adapter has its own tests; the hook is tested on the real binary, because other
  people's agents depend on it.
