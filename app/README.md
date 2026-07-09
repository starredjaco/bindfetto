# bindfetto control app (Track C)

An Android GUI that drives the bindfetto runtime's **in-kernel interface filter** over
its control channel. It lists the Binder interfaces bindfetto has observed and lets you
pick which ones to keep — the selection is pushed into the probe's BPF filter maps live,
so non-matching transactions are dropped **before the ring buffer** (cutting the observer
effect on the device under test).

## What it does

- **Refresh** — `LIST` (every interface seen so far) + `GET` (the active filter), shown as
  a checkbox list with the current filter pre-checked.
- **Apply filter** — `SET` the checked interfaces as the in-kernel filter.
- **Clear** — disable filtering (capture everything again).

All logic is a thin client over the runtime's line protocol; see
`app/src/main/java/com/bindfetto/control/ControlClient.kt`.

## Control channel

The app talks to bindfetto over TCP (default `127.0.0.1:3491`). Start the runtime with the
control server enabled:

```sh
adb shell /data/local/tmp/bindfetto --control 3491 --sink none
```

The app runs on-device and connects to `localhost`. For development from the host, the
same protocol is reachable via `adb forward tcp:3491 tcp:3491` and any TCP client.

## Build & install

Needs JDK 17+ (Android Studio's bundled JBR works) and the Android SDK. The Gradle
wrapper pins the Gradle version; dependencies download on first build.

```sh
export JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home"
cd app
./gradlew :app:assembleDebug
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

Then launch **bindfetto filter** from the launcher, tap **Refresh**, check interfaces, and
tap **Apply filter**.

## Scope

This first version does discovery + filtering only; it assumes bindfetto is already
running (started via adb). Lifecycle control (start/stop) and binary deployment
(signature permission / adb fallback) are future Track C work.
