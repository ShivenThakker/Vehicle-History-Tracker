"use client";

import {
  Contract,
  Networks,
  TransactionBuilder,
  Keypair,
  xdr,
  Address,
  nativeToScVal,
  scValToNative,
  rpc,
} from "@stellar/stellar-sdk";
import {
  isConnected,
  getAddress,
  signTransaction,
  setAllowed,
  isAllowed,
  requestAccess,
} from "@stellar/freighter-api";

// ============================================================
// CONSTANTS — Update these for your contract
// ============================================================

/** Your deployed Soroban contract ID */
export const CONTRACT_ADDRESS =
  "CA5FPUIBVP4KPUDLNK5LQJALOB4JDAKHM3TTAN5SDJWZ24VQ5XBWZRXX";

/** Network passphrase (testnet by default) */
export const NETWORK_PASSPHRASE = Networks.TESTNET;

/** Soroban RPC URL */
export const RPC_URL = "https://soroban-testnet.stellar.org";

/** Horizon URL */
export const HORIZON_URL = "https://horizon-testnet.stellar.org";

/** Network name for Freighter */
export const NETWORK = "TESTNET";

// ============================================================
// RPC Server Instance
// ============================================================

const server = new rpc.Server(RPC_URL);

// ============================================================
// Wallet Helpers
// ============================================================

export async function checkConnection(): Promise<boolean> {
  const result = await isConnected();
  return result.isConnected;
}

export async function connectWallet(): Promise<string> {
  const connResult = await isConnected();
  if (!connResult.isConnected) {
    throw new Error("Freighter extension is not installed or not available.");
  }

  const allowedResult = await isAllowed();
  if (!allowedResult.isAllowed) {
    await setAllowed();
    await requestAccess();
  }

  const { address } = await getAddress();
  if (!address) {
    throw new Error("Could not retrieve wallet address from Freighter.");
  }
  return address;
}

export async function getWalletAddress(): Promise<string | null> {
  try {
    const connResult = await isConnected();
    if (!connResult.isConnected) return null;

    const allowedResult = await isAllowed();
    if (!allowedResult.isAllowed) return null;

    const { address } = await getAddress();
    return address || null;
  } catch {
    return null;
  }
}

// ============================================================
// Contract Interaction Helpers
// ============================================================

/**
 * Build, simulate, and optionally sign + submit a Soroban contract call.
 *
 * @param method   - The contract method name to invoke
 * @param params   - Array of xdr.ScVal parameters for the method
 * @param caller   - The public key (G...) of the calling account
 * @param sign     - If true, signs via Freighter and submits. If false, only simulates.
 * @returns        The result of the simulation or submission
 */
export async function callContract(
  method: string,
  params: xdr.ScVal[] = [],
  caller: string,
  sign: boolean = true
) {
  const contract = new Contract(CONTRACT_ADDRESS);
  const account = await server.getAccount(caller);

  const tx = new TransactionBuilder(account, {
    fee: "100",
    networkPassphrase: NETWORK_PASSPHRASE,
  })
    .addOperation(contract.call(method, ...params))
    .setTimeout(30)
    .build();

  const simulated = await server.simulateTransaction(tx);

  if (rpc.Api.isSimulationError(simulated)) {
    throw new Error(
      `Simulation failed: ${(simulated as rpc.Api.SimulateTransactionErrorResponse).error}`
    );
  }

  if (!sign) {
    // Read-only call — just return the simulation result
    return simulated;
  }

  // Prepare the transaction with the simulation result
  const prepared = rpc.assembleTransaction(tx, simulated).build();

  // Sign with Freighter
  const { signedTxXdr } = await signTransaction(prepared.toXDR(), {
    networkPassphrase: NETWORK_PASSPHRASE,
  });

  const txToSubmit = TransactionBuilder.fromXDR(
    signedTxXdr,
    NETWORK_PASSPHRASE
  );

  const result = await server.sendTransaction(txToSubmit);

  if (result.status === "ERROR") {
    throw new Error(`Transaction submission failed: ${result.status}`);
  }

  // Poll for confirmation
  let getResult = await server.getTransaction(result.hash);
  while (getResult.status === "NOT_FOUND") {
    await new Promise((resolve) => setTimeout(resolve, 1000));
    getResult = await server.getTransaction(result.hash);
  }

  if (getResult.status === "FAILED") {
    throw new Error("Transaction failed on chain.");
  }

  return getResult;
}

/**
 * Read-only contract call (does not require signing).
 */
export async function readContract(
  method: string,
  params: xdr.ScVal[] = [],
  caller?: string
) {
  const account =
    caller || Keypair.random().publicKey(); // Use a random keypair for read-only
  const sim = await callContract(method, params, account, false);
  if (
    rpc.Api.isSimulationSuccess(sim as rpc.Api.SimulateTransactionResponse) &&
    (sim as rpc.Api.SimulateTransactionSuccessResponse).result
  ) {
    return scValToNative(
      (sim as rpc.Api.SimulateTransactionSuccessResponse).result!.retval
    );
  }
  return null;
}

// ============================================================
// ScVal Conversion Helpers
// ============================================================

export function toScValString(value: string): xdr.ScVal {
  return nativeToScVal(value, { type: "string" });
}

export function toScValU32(value: number): xdr.ScVal {
  return nativeToScVal(value, { type: "u32" });
}

export function toScValI128(value: bigint): xdr.ScVal {
  return nativeToScVal(value, { type: "i128" });
}

export function toScValAddress(address: string): xdr.ScVal {
  return new Address(address).toScVal();
}

export function toScValBool(value: boolean): xdr.ScVal {
  return nativeToScVal(value, { type: "bool" });
}

// ============================================================
// Supply Chain Tracker — Contract Methods
// ============================================================

export function toScValU64(value: number): xdr.ScVal {
  return nativeToScVal(value, { type: "u64" });
}

// ============================================================
// Vehicle History Tracker — Contract Methods
// ============================================================

/**
 * Initialize the contract with an admin.
 * Calls: initialize(admin: Address)
 */
export async function initializeContract(caller: string, adminAddress: string) {
  return callContract(
    "initialize",
    [toScValAddress(adminAddress)],
    caller,
    true
  );
}

/**
 * Register a new vehicle.
 * Calls: register_vehicle(owner: Address, vin: String, make: String, model: String, year: u32) -> Result
 */
export async function registerVehicle(
  caller: string,
  ownerAddress: string,
  vin: string,
  make: string,
  model: string,
  year: number
) {
  return callContract(
    "register_vehicle",
    [
      toScValAddress(ownerAddress),
      toScValString(vin),
      toScValString(make),
      toScValString(model),
      toScValU32(year),
    ],
    caller,
    true
  );
}

/**
 * Add a history event to a vehicle.
 * Calls: add_history_event(caller: Address, vin: String, event_type: String, description: String, mileage: u64) -> Result
 */
export async function addHistoryEvent(
  caller: string,
  vin: string,
  eventType: string,
  description: string,
  mileage: number
) {
  return callContract(
    "add_history_event",
    [
      toScValAddress(caller),
      toScValString(vin),
      toScValString(eventType),
      toScValString(description),
      toScValU64(mileage),
    ],
    caller,
    true
  );
}

/**
 * Transfer vehicle ownership.
 * Calls: transfer_ownership(vin: String, new_owner: Address) -> Result
 */
export async function transferOwnership(
  caller: string,
  vin: string,
  newOwnerAddress: string
) {
  return callContract(
    "transfer_ownership",
    [toScValString(vin), toScValAddress(newOwnerAddress)],
    caller,
    true
  );
}

/**
 * Mark a vehicle as stolen (admin only).
 * Calls: mark_stolen(vin: String) -> Result
 */
export async function markStolen(caller: string, vin: string) {
  return callContract(
    "mark_stolen",
    [toScValString(vin)],
    caller,
    true
  );
}

/**
 * Clear stolen flag (admin only).
 * Calls: clear_stolen(vin: String) -> Result
 */
export async function clearStolen(caller: string, vin: string) {
  return callContract(
    "clear_stolen",
    [toScValString(vin)],
    caller,
    true
  );
}

/**
 * Get vehicle details (read-only).
 * Calls: get_vehicle(vin: String) -> Option<Vehicle>
 * Returns: { vin, make, model, year, owner, is_stolen, total_events } or null
 */
export async function getVehicle(vin: string, caller?: string) {
  return readContract(
    "get_vehicle",
    [toScValString(vin)],
    caller
  );
}

/**
 * Get vehicle history (read-only).
 * Calls: get_history(vin: String) -> Vec<HistoryEvent>
 * Returns: Array of events
 */
export async function getHistory(vin: string, caller?: string) {
  return readContract(
    "get_history",
    [toScValString(vin)],
    caller
  );
}

/**
 * Get a single history event by index (read-only).
 * Calls: get_event(vin: String, index: u32) -> Option<HistoryEvent>
 */
export async function getEvent(vin: string, index: number, caller?: string) {
  return readContract(
    "get_event",
    [toScValString(vin), toScValU32(index)],
    caller
  );
}

/**
 * Check if vehicle is stolen (read-only).
 * Calls: is_stolen(vin: String) -> bool
 */
export async function isStolen(vin: string, caller?: string) {
  return readContract(
    "is_stolen",
    [toScValString(vin)],
    caller
  );
}

/**
 * Get total number of history events (read-only).
 * Calls: total_events(vin: String) -> u32
 */
export async function getTotalEvents(vin: string, caller?: string) {
  return readContract(
    "total_events",
    [toScValString(vin)],
    caller
  );
}

// ============================================================
// Legacy Supply Chain aliases (for backward compatibility)
// ============================================================

export async function addProduct(caller: string, productId: string, origin: string) {
  // Not applicable for vehicle tracker - use registerVehicle instead
  throw new Error("Use registerVehicle for vehicle registration");
}

export async function updateProductStatus(caller: string, productId: string, newStatus: string) {
  // Not applicable for vehicle tracker - use addHistoryEvent instead
  throw new Error("Use addHistoryEvent to add vehicle service history");
}

export async function getProduct(productId: string, caller?: string) {
  // Alias for getVehicle
  return getVehicle(productId, caller);
}

export { nativeToScVal, scValToNative, Address, xdr };
