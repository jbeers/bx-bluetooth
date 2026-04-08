import init, { PrinterHarnessVM } from "./pkg/printer_harness.js";

const DISPLAY_TSPL = [
  "SIZE 58 mm, 40 mm",
  "GAP 0,0",
  "DIRECTION 0",
  "REFERENCE 0,0",
  "CLS",
  "TEXT 20,20,\"3\",0,1,1,\"BX BLUETOOTH OK\"",
  "TEXT 20,60,\"2\",0,1,1,\"MATCHBOX WASM TEST\"",
  "PRINT 1",
  "END"
].join("\r\n");

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
  vm: null,
  characteristics: [],
  selectedIndex: 0
};

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

function setConnectionStatus(value) {
  els.connectionStatus.textContent = value;
}

function syncButtons() {
  const hasDevice = els.deviceId.textContent !== "None";
  const connected = state.characteristics.length > 0;
  const hasSelection = Number(els.characteristics.value) >= 0;

  els.connectDiscover.disabled = !hasDevice;
  els.disconnect.disabled = !connected;
  els.sendPrint.disabled = !(connected && hasSelection);
}

function clearCharacteristics() {
  state.characteristics = [];
  state.selectedIndex = 0;
  els.characteristics.innerHTML = "";
  syncButtons();
}

function renderCharacteristics(characteristics, selectedIndex) {
  state.characteristics = characteristics;
  state.selectedIndex = selectedIndex;
  els.characteristics.innerHTML = "";

  characteristics.forEach((entry, zeroIndex) => {
    const option = document.createElement("option");
    const props = [];
    if (entry.writeWithoutResponse) {
      props.push("withoutResponse");
    }
    if (entry.write) {
      props.push("withResponse");
    }
    option.value = String(zeroIndex);
    option.textContent = `${entry.serviceUuid} :: ${entry.uuid} [${props.join(", ")}]`;
    els.characteristics.append(option);
  });

  if (characteristics.length > 0) {
    const zeroIndex = Math.max(0, selectedIndex - 1);
    els.characteristics.value = String(zeroIndex);
  }

  syncButtons();
}

function updateDevice(device) {
  els.deviceName.textContent = device?.name || "None";
  els.deviceId.textContent = device?.id || "None";
  syncButtons();
}

function updateFromState(snapshot) {
  updateDevice(snapshot.device);
  if (Array.isArray(snapshot.characteristics) && snapshot.characteristics.length > 0) {
    renderCharacteristics(snapshot.characteristics, snapshot.selectedCharacteristicIndex || 1);
    setConnectionStatus("Connected and discovered");
  } else {
    clearCharacteristics();
  }
}

function configureVm() {
  const optionalServices = parseUuidList(els.serviceUuids.value);
  const preferredCharacteristic = normalizeUuid(els.preferredCharacteristic.value);
  const snapshot = state.vm.configure(
    els.namePrefix.value.trim(),
    optionalServices,
    preferredCharacteristic,
    els.writeMode.value
  );

  els.payload.value = `${DISPLAY_TSPL}\n\n// payload bytes: ${snapshot.payloadLength}`;
  return snapshot;
}

async function boot() {
  log("Loading wasm package...");
  await init();
  log("Wasm package loaded.");
  log("Creating Matchbox harness VM...");
  state.vm = new PrinterHarnessVM();
  log("Matchbox harness VM created.");
  const snapshot = configureVm();
  updateFromState(snapshot);
  setConnectionStatus("Ready");
  log("Matchbox wasm harness initialized.");
}

els.requestDevice.addEventListener("click", async () => {
  try {
    if (!state.vm) {
      throw new Error("Harness VM is not available.");
    }
    configureVm();
    log("Requesting printer through BoxLang module...");
    const device = await state.vm.request_printer();
    updateDevice(device);
    clearCharacteristics();
    setConnectionStatus("Device selected");
    log(`Selected device ${device.name || "Unnamed device"} (${device.id}).`);
  } catch (error) {
    log(`requestPrinter failed: ${error?.message || String(error)}`);
  }
});

els.connectDiscover.addEventListener("click", async () => {
  try {
    if (!state.vm) {
      throw new Error("Harness VM is not available.");
    }
    configureVm();
    log("Connecting and discovering through BoxLang module...");
    const result = await state.vm.connect_and_discover();
    renderCharacteristics(result.characteristics || [], result.selectedIndex || 1);
    setConnectionStatus("Connected and discovered");
    log(`Discovered ${state.characteristics.length} writable characteristic(s).`);
  } catch (error) {
    log(`connectAndDiscover failed: ${error?.message || String(error)}`);
    clearCharacteristics();
    setConnectionStatus("Discovery failed");
  }
});

els.characteristics.addEventListener("change", () => {
  try {
    if (!state.vm) {
      throw new Error("Harness VM is not available.");
    }
    const oneBasedIndex = Number(els.characteristics.value) + 1;
    if (oneBasedIndex > 0) {
      const selected = state.vm.select_characteristic(oneBasedIndex);
      state.selectedIndex = oneBasedIndex;
      log(`Selected characteristic ${selected.uuid}.`);
    }
    syncButtons();
  } catch (error) {
    log(`selectCharacteristic failed: ${error?.message || String(error)}`);
  }
});

els.sendPrint.addEventListener("click", async () => {
  try {
    if (!state.vm) {
      throw new Error("Harness VM is not available.");
    }
    configureVm();
    const selectedIndex = Number(els.characteristics.value);
    if (!Number.isNaN(selectedIndex) && selectedIndex >= 0) {
      state.vm.select_characteristic(selectedIndex + 1);
    }
    log("Sending hardcoded TSPL payload through BoxLang module...");
    const result = await state.vm.send_test_print();
    log(`Sent ${result.bytesSent} byte(s) to ${result.characteristic.uuid}.`);
  } catch (error) {
    log(`sendTestPrint failed: ${error?.message || String(error)}`);
  }
});

els.disconnect.addEventListener("click", async () => {
  try {
    if (!state.vm) {
      throw new Error("Harness VM is not available.");
    }
    await state.vm.disconnect_printer();
    clearCharacteristics();
    updateDevice(null);
    setConnectionStatus("Disconnected");
    log("Disconnected.");
  } catch (error) {
    log(`disconnect failed: ${error?.message || String(error)}`);
  }
});

boot().catch((error) => {
  setConnectionStatus("Boot failed");
  syncButtons();
  log(`boot failed: ${error?.stack || error?.message || String(error)}`);
});
