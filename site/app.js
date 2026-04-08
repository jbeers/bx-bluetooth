const DEFAULT_TSPL = [
  "SIZE 58 mm, 40 mm",
  "GAP 0,0",
  "DIRECTION 0",
  "REFERENCE 0,0",
  "CLS",
  "TEXT 20,20,\"3\",0,1,1,\"BX BLUETOOTH OK\"",
  "TEXT 20,60,\"2\",0,1,1,\"WEB BLUETOOTH TEST\"",
  "PRINT 1",
  "END",
  ""
].join("\r\n");

const CHUNK_SIZE = 128;
const CHUNK_DELAY_MS = 20;

const els = {
  namePrefix: document.querySelector("#name-prefix"),
  serviceUuids: document.querySelector("#service-uuids"),
  preferredCharacteristic: document.querySelector("#preferred-characteristic"),
  writeMode: document.querySelector("#write-mode"),
  requestDevice: document.querySelector("#request-device"),
  connectDiscover: document.querySelector("#connect-discover"),
  sendPrint: document.querySelector("#send-print"),
  disconnect: document.querySelector("#disconnect"),
  payload: document.querySelector("#payload"),
  deviceName: document.querySelector("#device-name"),
  deviceId: document.querySelector("#device-id"),
  connectionStatus: document.querySelector("#connection-status"),
  characteristics: document.querySelector("#characteristics"),
  log: document.querySelector("#log")
};

const state = {
  device: null,
  server: null,
  writableCharacteristics: []
};

els.payload.value = DEFAULT_TSPL;

function log(message) {
  const timestamp = new Date().toLocaleTimeString();
  els.log.textContent += `[${timestamp}] ${message}\n`;
  els.log.scrollTop = els.log.scrollHeight;
}

function normalizeUuid(input) {
  const raw = input.trim().toLowerCase();
  if (!raw) {
    return "";
  }
  if (raw.length === 4) {
    return `0000${raw}-0000-1000-8000-00805f9b34fb`;
  }
  if (raw.length === 8) {
    return `${raw}-0000-1000-8000-00805f9b34fb`;
  }
  return raw;
}

function parseUuidList(input) {
  return input
    .split(",")
    .map((value) => normalizeUuid(value))
    .filter(Boolean);
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function selectedCharacteristic() {
  const index = Number(els.characteristics.value);
  if (Number.isNaN(index) || index < 0) {
    return null;
  }
  return state.writableCharacteristics[index] ?? null;
}

function setConnectionStatus(value) {
  els.connectionStatus.textContent = value;
}

function syncButtons() {
  const hasDevice = Boolean(state.device);
  const hasServer = Boolean(state.server?.connected);
  const hasCharacteristic = Boolean(selectedCharacteristic());

  els.connectDiscover.disabled = !hasDevice;
  els.disconnect.disabled = !hasServer;
  els.sendPrint.disabled = !(hasServer && hasCharacteristic);
}

function clearCharacteristics() {
  state.writableCharacteristics = [];
  els.characteristics.innerHTML = "";
  syncButtons();
}

function renderCharacteristics() {
  els.characteristics.innerHTML = "";

  state.writableCharacteristics.forEach((entry, index) => {
    const option = document.createElement("option");
    const props = [];
    if (entry.characteristic.properties.writeWithoutResponse) {
      props.push("withoutResponse");
    }
    if (entry.characteristic.properties.write) {
      props.push("withResponse");
    }
    option.value = String(index);
    option.textContent = `${entry.serviceUuid} :: ${entry.uuid} [${props.join(", ")}]`;
    els.characteristics.append(option);
  });

  const preferredUuid = normalizeUuid(els.preferredCharacteristic.value);
  const preferredIndex = state.writableCharacteristics.findIndex(
    (entry) => entry.uuid === preferredUuid
  );

  if (preferredIndex >= 0) {
    els.characteristics.value = String(preferredIndex);
    log(`Auto-selected preferred characteristic ${preferredUuid}.`);
  } else if (state.writableCharacteristics.length > 0) {
    els.characteristics.value = "0";
    log("Preferred characteristic not found. Selected the first writable characteristic.");
  }

  syncButtons();
}

function setDevice(device) {
  state.device = device;
  state.server = null;
  clearCharacteristics();

  els.deviceName.textContent = device?.name || "Unnamed device";
  els.deviceId.textContent = device?.id || "Unavailable";
  setConnectionStatus(device ? "Device selected" : "Idle");
  syncButtons();
}

function resetConnectionState(reason) {
  state.server = null;
  clearCharacteristics();
  setConnectionStatus(reason);
  syncButtons();
}

async function requestPrinter() {
  if (!navigator.bluetooth) {
    throw new Error("Web Bluetooth is not available in this browser.");
  }

  const namePrefix = els.namePrefix.value.trim();
  const optionalServices = parseUuidList(els.serviceUuids.value);
  const requestOptions = {};

  if (namePrefix) {
    const filter = {};
    if (namePrefix) {
      filter.namePrefix = namePrefix;
    }
    requestOptions.filters = [filter];
  } else {
    requestOptions.acceptAllDevices = true;
  }

  if (optionalServices.length > 0) {
    requestOptions.optionalServices = optionalServices;
  }

  log(`Requesting device with ${JSON.stringify(requestOptions)}.`);
  const device = await navigator.bluetooth.requestDevice(requestOptions);
  device.addEventListener("gattserverdisconnected", () => {
    log("Device disconnected.");
    resetConnectionState("Disconnected");
  });
  return device;
}

async function collectServicesWithFallback(server, optionalServices) {
  const primaryServices = await server.getPrimaryServices();
  if (primaryServices.length > 0) {
    return primaryServices;
  }

  const recovered = [];
  for (const uuid of optionalServices) {
    try {
      const service = await server.getPrimaryService(uuid);
      recovered.push(service);
      log(`Recovered service ${service.uuid} via targeted lookup.`);
    } catch (error) {
      log(`Primary service ${uuid} was not available: ${error.message}`);
    }
  }
  return recovered;
}

async function connectAndDiscover() {
  if (!state.device) {
    throw new Error("No device has been selected yet.");
  }

  log(`Connecting to ${state.device.name || state.device.id}...`);
  try {
    state.server = await state.device.gatt.connect();
  } catch (error) {
    log(`Initial connect failed: ${error.message}. Retrying once...`);
    await sleep(500);
    state.server = await state.device.gatt.connect();
  }
  setConnectionStatus("Connected");

  const optionalServices = parseUuidList(els.serviceUuids.value);
  log("Discovering primary services...");
  const services = await collectServicesWithFallback(state.server, optionalServices);
  log(`Found ${services.length} primary services.`);

  const entries = [];

  for (const service of services) {
    log(`Discovering characteristics for ${service.uuid}...`);
    const characteristics = await service.getCharacteristics();
    for (const characteristic of characteristics) {
      const writable =
        characteristic.properties.write || characteristic.properties.writeWithoutResponse;
      if (!writable) {
        continue;
      }

      const entry = {
        serviceUuid: service.uuid.toLowerCase(),
        uuid: characteristic.uuid.toLowerCase(),
        characteristic
      };

      entries.push(entry);
      log(
        `Writable characteristic ${entry.uuid} on ${entry.serviceUuid} ` +
          `(write=${characteristic.properties.write}, withoutResponse=${characteristic.properties.writeWithoutResponse})`
      );
    }
  }

  state.writableCharacteristics = entries;
  renderCharacteristics();

  if (entries.length === 0) {
    throw new Error("No writable characteristics were discovered.");
  }
}

async function writePayload(characteristic, bytes) {
  const mode = els.writeMode.value;
  const useWithoutResponse =
    mode === "withoutResponse" && characteristic.properties.writeWithoutResponse;
  const useWithResponse =
    mode === "withResponse" && characteristic.properties.write;

  if (!useWithoutResponse && !useWithResponse) {
    throw new Error(`Selected characteristic does not support ${mode}.`);
  }

  for (let offset = 0; offset < bytes.length; offset += CHUNK_SIZE) {
    const chunk = bytes.slice(offset, offset + CHUNK_SIZE);
    if (useWithoutResponse) {
      await characteristic.writeValueWithoutResponse(chunk);
    } else {
      await characteristic.writeValueWithResponse(chunk);
    }
    log(`Sent chunk ${Math.floor(offset / CHUNK_SIZE) + 1} (${chunk.length} bytes).`);
    if (useWithoutResponse && offset + CHUNK_SIZE < bytes.length) {
      await sleep(CHUNK_DELAY_MS);
    }
  }
}

els.requestDevice.addEventListener("click", async () => {
  try {
    const device = await requestPrinter();
    setDevice(device);
    log(`Selected device ${device.name || "Unnamed device"} (${device.id}).`);
  } catch (error) {
    log(`requestDevice failed: ${error.message}`);
  }
});

els.connectDiscover.addEventListener("click", async () => {
  try {
    clearCharacteristics();
    await connectAndDiscover();
    setConnectionStatus("Connected and discovered");
    log("Discovery complete.");
  } catch (error) {
    log(`connectAndDiscover failed: ${error.message}`);
    if (state.server?.connected) {
      state.server.disconnect();
    }
    resetConnectionState("Discovery failed");
  }
});

els.characteristics.addEventListener("change", () => {
  syncButtons();
});

els.sendPrint.addEventListener("click", async () => {
  try {
    const entry = selectedCharacteristic();
    if (!entry) {
      throw new Error("No writable characteristic is selected.");
    }

    const bytes = new TextEncoder().encode(els.payload.value);
    log(`Sending ${bytes.length} bytes to ${entry.uuid}...`);
    await writePayload(entry.characteristic, bytes);
    log("Test print payload sent.");
  } catch (error) {
    log(`print failed: ${error.message}`);
  }
});

els.disconnect.addEventListener("click", () => {
  if (state.server?.connected) {
    state.server.disconnect();
    log("Disconnect requested.");
  } else {
    resetConnectionState("Disconnected");
  }
});

if (!window.isSecureContext) {
  log("This page is not running in a secure context. Web Bluetooth will not work.");
}
if (!navigator.bluetooth) {
  log("navigator.bluetooth is unavailable in this browser.");
}
