# verandah-plugin-pomodoro

A pomodoro timer widget plugin for [verandah](https://git.sw1nn.net/sw1nn/verandah).

## Features

- Configurable work/break durations
- Multiple render modes for visual feedback
- Sound notifications on phase transitions
- External control via socket (`verandah-pomodoroctl`)

## Configuration

```toml
[[keys]]
index = 2
[keys.widget]
id = "pomodoro"
icons = { work = "pomodoro", short_break = "pomodoro-coffee", long_break = "pomodoro-beach-umbrella" }
[keys.widget.config]
work = 25
short_break = 5
long_break = 15
render_mode = "ripen"
colors = { fg = "#ecf0f1", paused_bg = "#34495e" }
sounds = { work = "alarm-clock-elapsed", break = "complete" }
[keys.action]
exec = "verandah-pomodoroctl toggle"
```

### Timer Settings

| Option | Default | Description |
|--------|---------|-------------|
| `work` | 25 | Work phase duration in minutes |
| `short_break` | 5 | Short break duration in minutes |
| `long_break` | 15 | Long break duration in minutes |

### Render Modes

The `render_mode` option controls how the timer is displayed:

#### `text` (default)

Traditional text-based display showing the countdown timer with phase indicator and iteration dots.

#### `fill_bg`

Displays progress as a fill from bottom to top (or top to bottom). The background fills with the phase color as time progresses.

Options:
- `fill_direction`: `"empty_to_full"` (default) or `"full_to_empty"`
- `empty_bg`: Background color for unfilled portion

#### `fill_icon`

Fills the phase icon from bottom to top as a progress indicator. The unfilled portion is shown in greyscale, creating a visual effect where the icon progressively colorizes as time passes.

Options:
- `fill_direction`: `"empty_to_full"` (default) or `"full_to_empty"`

Requires icons to be configured. Falls back to `fill_bg` if no icon is available.

#### `ripen`

Icon starts with a green tint (unripe) and gradually returns to its original colors as the timer progresses. Uses hue rotation in HSL color space to shift all colors towards green at the start, creating a "ripening fruit" effect where the icon becomes more vibrant as time passes.

Requires icons to be configured.

### Colors

```toml
colors = { fg = "#ecf0f1", work_bg = "#e57373", break_bg = "#81c784", paused_bg = "#34495e", empty_bg = "#2c3e50" }
```

| Color | Default | Description |
|-------|---------|-------------|
| `fg` | `#ffffff` | Foreground/text color |
| `work_bg` | `#e57373` | Background during work phase |
| `break_bg` | `#81c784` | Background during break phase |
| `paused_bg` | `#7f8c8d` | Background when paused |
| `empty_bg` | `#2c3e50` | Unfilled background in filling modes |

### Sounds

```toml
sounds = { work = "alarm-clock-elapsed", break = "complete" }
```

Sound names are resolved via XDG sound theme directories. The `work` sound plays when a work phase completes, and `break` plays when a break phase completes.

## Control

Use `verandah-pomodoroctl` to control the timer:

```bash
verandah-pomodoroctl toggle  # Start/stop the timer
verandah-pomodoroctl reset   # Reset to beginning
verandah-pomodoroctl skip    # Skip to next phase
verandah-pomodoroctl start   # Start the timer
verandah-pomodoroctl stop    # Stop/pause the timer
```

## License

MIT
