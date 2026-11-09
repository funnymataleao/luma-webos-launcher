# Tech weather headlines. Every line below is loaded as-is.

# Format: Title — subtitle. {N},{old},{new},{delta} are substituted.

# The highlighted title word is the first one matching a tech keyword (cpu, cache, kernel, ...), see view.rs.

## clear_nice (clear/pleasant, 15–28 °C)

- All systems nominal. — Clear skies, zero packet loss outside.
- Uptime: 100%. — Not a cloud in the stack trace.
- Sunlight.exe is running. — Go outside. It's a feature, not a bug.
- Perfect conditions, zero latency. — Rare build. Don't touch anything.
- Weather: compiled without warnings. — Ship it. Then go outside.

## chilly (5–14 °C)

- Thermal throttling: off. — Hoodie weather. Grab one from the merch drawer.
- Running a little cool. — Perfect temps for a long build.
- Fans at idle speed. — It's brisk out. Layer up.
- Optimal CPU temps outside. — Less optimal for humans. Jacket.

## cold (0–4 °C)

- Cold boot conditions. — Let your fingers warm up before typing.
- Server room cold. — Bring a sweater to the data center.
- Heatsink not required. — It's freezing out there.
- Liquid cooling, minus the liquid. — Just cold. Really cold.
- Running at 2% CPU warmth. — Hot coffee recommended.

## frost (below 0 °C)

- Kernel frozen. — Below zero. Nothing is responding, including you.
- Cryo mode activated. — Below freezing. Stay in and stream something.
- Overclocking weather. — Below zero. Your GPU would love it out there.
- Ice-cold cache. — Frost outside. Everything is slow to start.
- System halted: frost. — Wake me up when it's above zero.

## hot (25 °C+)

- CPU at 95°C. — Thermal throttling in effect. Stay inside.
- Thermal runaway. — It's an oven out there. Find the AC.
- Overheating detected. — Hot enough to reflow solder.
- Fans at 100%. — Hydrate like your power supply depends on it.
- Global warming patch failed. — Too hot. Stay near the AC.
- Hotfix needed: sun. — Hot as a gaming laptop on your lap.

## cloudy

- Everything's in the cloud. — Grey skies, no sun available.
- Grey screen of meh. — Perfect weather for screen time.
- Cloud storage: full. — Grey skies all day.
- Low-contrast mode. — Grey outside, so the TV is the brightest thing around.
- Sun.dll not found. — Overcast. Try again later.

## rain

- Water-cooled outside. — It's raining. Keep the electronics indoors.
- Packet loss: rain. — It's raining. Stay on the wired connection.
- Liquid damage risk: high. — Rain outside. Umbrella recommended.
- Rain.exe has started. — It's raining. Stay in and binge something.
- Downloading rain... — 100% complete. Look outside.

## thunder

- Power surge warning. — Thunderstorms. Unplug the expensive stuff.
- Lightning-fast connection. — Literally. Storms overhead.
- Check your UPS. — Thunderstorms nearby. Save your work.

## snow (snow and sleet)

- White noise outside. — Snow everywhere.
- Pixel-perfect snowfall. — It's snowing. Enjoy the render.
- Snow.exe is buffering. — Don't go out. Let it finish.
- Dead pixels everywhere. — Oh wait, it's snow.
- Freeze frame. — Snow outside. Perfect weather for a movie.

## wind

- Max fan speed outside. — It's windy. Hold on to your headphones.
- Airflow: excessive. — Strong winds. Secure the satellite dish.
- High bandwidth breeze. — Gusty out there.

## fog

- Low resolution mode. — Foggy. Visibility like 240p.
- Rendering distance: 2 m. — Foggy outside. Lower your expectations.
- Blur filter enabled. — Thick fog. Can't see a thing.

## night (late evening/night, no precipitation)

- Dark mode activated. — It's late. One more episode?
- Entering sleep mode. — It's late. Even servers need maintenance windows.
- Night shift engaged. — Go to bed. The backups can run without you.

## still (the same category holds for 2+ days in a row)

- Still cold, still no patch. — Day {N}. Spring is still in beta.
- Still raining. Infinite loop. — Day {N}. Build an ark. Or a server rack.
- Still grey. Error 404: sun. — The sun is still in development.
- Still hot. CPU still throttling. — Day {N}. The AC is your only friend.

## change (sudden change: new category or Δt ≥ 6 °C in ~3 h)

- Unexpected weather update. — Was {old} an hour ago.
- Hot-swapped forecast. — {old} → {new}.
- Breaking changes detected. — Dress for every version.
- It got colder: rollback. — {delta}° down since this morning.
