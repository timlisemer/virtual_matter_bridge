# Home Assistant Yellow test, 2026-10-04

The `matter-1.5-cleanup` bridge uses rs-matter revision
`578ae0e0b875edfc3ffec80e94aa0ff48fb31cc9` (Matter 1.5.1). The live controller
uses matterjs-server 1.4.0 and Home Assistant 2026.9.4.

## Live results

- Commissioning succeeded on Yellow through its existing Matter controller.
- All four W100 devices show temperature, humidity, battery, and three named
  buttons in Home Assistant. Initial measurements come from the Zigbee2MQTT
  state cache. No initial measurement is invented.
- Each of the twelve W100 buttons received single, double, hold, and release
  actions through MQTT and Matter. Home Assistant received all 48 expected
  events. These were simulated MQTT actions, not physical button presses.
- Both Shelly 2PM devices expose all four relay channels with their code names.
  Home Assistant receives their live states and standard electrical sensors.
- No Shelly switch command was sent. The real relays were not operated for the
  test. Only the simulated Power Strip was switched off and back on through
  Home Assistant.
- The simulated doorbell press produced a Home Assistant Matter button event.
- The simulated H.264/AAC stream played in the Home Assistant browser at
  640 by 360 pixels. Video uses Generic Camera, since Home Assistant has no
  Matter camera platform for this endpoint.
- Bridge restart keeps the commissioned fabric. Schema changes request a new
  interview instead of deleting the fabric credentials.

## Naming and controller limits

Bridged Device Basic Information supplies real device names. Fixed Label
entries supply button labels and repeated endpoint labels. Home Assistant
suppresses Fixed Label text on primary bridge switch entities. The NixOS
configuration supplies explicit friendly names for the simulated outlets.
An internal entity ID suffix does not supply the displayed label.

Home Assistant does not create native sensors for Matter frequency, power
factor, or the custom Shelly diagnostics cluster. The bridge publishes these
attributes, but their absence from the UI is a controller limitation.

## Simulation

Publish the non-retained payload `press` to
`virtual-matter-bridge/doorbell/press`. Do not publish a simulated action to a
real Shelly command topic.

The NixOS test stream is `rtsp://vmb-doorbell-stream:8554/doorbell` inside the
Home Assistant Docker network. The camera dashboard includes the stream and
the Matter press event. It does not control the real Reolink doorbell.

## Checks

`just check` passed in both repositories. The astral-ai check MCP was called,
but its session hook was unavailable. The direct repository checks supplied
the validation result. The NixOS package and system build provide deployment
validation in addition to those checks.
