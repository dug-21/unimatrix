#!/bin/sh
# Ensures Docker CE works inside the devcontainer.
# Docker CE requires a separate containerd process; the DinD feature's
# init script only starts dockerd, so containerd may not be running.
# The root filesystem is overlay, so overlay-on-overlay fails —
# daemon.json (baked into the image) sets the vfs storage driver.

# Start containerd if not running
if ! pgrep -x containerd > /dev/null 2>&1; then
    sudo containerd > /dev/null 2>&1 &
    sleep 2
fi

# Start dockerd if not responding
if ! docker info > /dev/null 2>&1; then
    sudo pkill dockerd 2>/dev/null || true
    sleep 1
    sudo dockerd > /dev/null 2>&1 &
    for i in $(seq 1 15); do
        docker info > /dev/null 2>&1 && break
        sleep 1
    done
fi

# Disable devpod credential helper (its agent isn't running, so pulls fail)
if [ -f /usr/local/bin/docker-credential-devpod ]; then
    sudo mv /usr/local/bin/docker-credential-devpod /usr/local/bin/docker-credential-devpod.disabled 2>/dev/null || true
fi
if grep -q credsStore ~/.docker/config.json 2>/dev/null; then
    echo '{"auths":{}}' > ~/.docker/config.json
fi
