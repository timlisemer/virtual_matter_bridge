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
- The automated tests sent no Shelly switch commands. Only the simulated Power
  Strip was switched off and back on through Home Assistant. Separate Tim-account
  calls to `light.buro_licht` appeared in the Home Assistant logbook at 21:40;
  Tim confirmed that he made those calls manually. They were outside these tests.
- The simulated doorbell press produced a Home Assistant Matter button event.
- The simulated H.264/AAC stream played in the Home Assistant browser at
  640 by 360 pixels. Video uses Generic Camera, since Home Assistant has no
  Matter camera platform for this endpoint.
- After the final NixOS activation and Home Assistant restart, the saved camera
  dashboard played the video with advancing playback time and no player error.
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

The NixOS `Virtual Matter Bridge` dashboard shows the Matter entities and
read-only MQTT companion values for frequency, power factor, and network
diagnostics. Its Shelly state cards have no switch action.

## Simulation

Publish the non-retained payload `press` to
`virtual-matter-bridge/doorbell/press`. Do not publish a simulated action to a
real Shelly command topic.

The NixOS test stream is `rtsp://vmb-doorbell-stream:8554/doorbell` inside the
Home Assistant Docker network. The camera dashboard includes the stream and
the Matter press event. It does not control the real Reolink doorbell.
The dashboard's `Simulated Doorbell Press` helper publishes only to the
simulation topic.

The deployed camera card selects Home Assistant's HLS player directly. The
stock live camera card selects WebRTC and reports that go2rtc does not support
this stream source. The custom card loads the normal Home Assistant camera
components and does not require a frontend bundle URL or an access token in
the configuration. It supports only `camera.vmb_doorbell_stream`.

## Checks

`just check` passed in both repositories. The astral-ai check MCP was called,
but its session hook was initially unavailable. After the local server rebuild,
the MCP returned `Transport closed`. The direct repository checks supplied
the validation result.

The release package was built natively on Yellow. Its test phase passed 140
tests with no failures. The full NixOS system build passed. The permanent
bridge, stream server, and video producer started under NixOS. The bridge
loaded its existing fabric and all four W100 state caches after activation.
The final deployed system is NixOS generation 344. The camera card JavaScript
syntax check and the final NixOS `just check` both passed.

The unrelated `docker-astral-ai-telemetry` service already failed before this
deployment because its `latest` image tag was missing. This causes a NixOS
activation warning. It does not prevent the bridge or Home Assistant from
running.
