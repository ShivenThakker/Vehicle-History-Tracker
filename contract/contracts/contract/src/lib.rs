#![no_std]
 
use soroban_sdk::{
    contract, contractimpl, contracttype, contracterror, symbol_short,
    Address, Env, String, Symbol, Vec,
};
 
// ─────────────────────────────────────────────
//  Storage key namespace
// ─────────────────────────────────────────────
const VEHICLE: Symbol = symbol_short!("VEHICLE");
const HISTORY: Symbol = symbol_short!("HISTORY");
const ADMIN:   Symbol = symbol_short!("ADMIN");
 
// ─────────────────────────────────────────────
//  Data types
// ─────────────────────────────────────────────
 
/// Core vehicle metadata stored on-chain.
#[contracttype]
#[derive(Clone, Debug)]
pub struct Vehicle {
    /// 17-character Vehicle Identification Number (VIN)
    pub vin:          String,
    pub make:         String,
    pub model:        String,
    pub year:         u32,
    pub owner:        Address,
    pub is_stolen:    bool,
    pub total_events: u32,
}
 
/// A single recorded event in the vehicle's life.
#[contracttype]
#[derive(Clone, Debug)]
pub struct HistoryEvent {
    pub event_type:  String,   // e.g. "SERVICE", "ACCIDENT", "OWNERSHIP_TRANSFER"
    pub description: String,
    pub mileage:     u64,
    pub timestamp:   u64,      // ledger timestamp (seconds)
    pub recorded_by: Address,  // who submitted this record
}
 
/// Errors the contract can emit.
#[contracterror]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum VehicleError {
    AlreadyRegistered  = 1,
    VehicleNotFound    = 2,
    Unauthorized       = 3,
    InvalidMileage     = 4,
    VehicleIsStolen    = 5,
}
 
// ─────────────────────────────────────────────
//  Contract
// ─────────────────────────────────────────────
 
#[contract]
pub struct VehicleHistoryTracker;
 
#[contractimpl]
#[allow(deprecated)]
impl VehicleHistoryTracker {
 
    // ── Initialise ───────────────────────────
 
    /// Deploy-time setup — stores the contract admin.
    pub fn initialize(env: Env, admin: Address) {
        admin.require_auth();
        env.storage().instance().set(&ADMIN, &admin);
    }
 
    // ── Vehicle registration ─────────────────
 
    /// Register a brand-new vehicle.
    /// Only callable by the vehicle owner (requires their signature).
    pub fn register_vehicle(
        env:   Env,
        owner: Address,
        vin:   String,
        make:  String,
        model: String,
        year:  u32,
    ) -> Result<(), VehicleError> {
        owner.require_auth();
 
        // Prevent double-registration
        if env.storage().persistent().has(&vin) {
            return Err(VehicleError::AlreadyRegistered);
        }
 
        let vehicle = Vehicle {
            vin:          vin.clone(),
            make,
            model,
            year,
            owner,
            is_stolen:    false,
            total_events: 0,
        };
 
        env.storage().persistent().set(&vin, &vehicle);
 
        // Initialise an empty history list for this VIN
        let history: Vec<HistoryEvent> = Vec::new(&env);
        let history_key = (HISTORY, vin.clone());
        env.storage().persistent().set(&history_key, &history);
 
        // Emit registration event
        env.events().publish(
            (VEHICLE, symbol_short!("v_reg")),
            vin,
        );
 
        Ok(())
    }
 
    // ── History events ───────────────────────
 
    /// Add a history event to a vehicle.
    /// Caller must be either the current owner OR the contract admin.
    pub fn add_history_event(
        env:         Env,
        caller:      Address,
        vin:         String,
        event_type:  String,
        description: String,
        mileage:     u64,
    ) -> Result<(), VehicleError> {
        caller.require_auth();
 
        // Load vehicle
        let mut vehicle: Vehicle = env
            .storage()
            .persistent()
            .get(&vin)
            .ok_or(VehicleError::VehicleNotFound)?;
 
        // Only owner or admin may write records
        let admin: Address = env.storage().instance().get(&ADMIN).unwrap();
        if caller != vehicle.owner && caller != admin {
            return Err(VehicleError::Unauthorized);
        }
 
        // Mileage must be non-decreasing (catches typos / fraud)
        let history_key = (HISTORY, vin.clone());
        let mut history: Vec<HistoryEvent> = env
            .storage()
            .persistent()
            .get(&history_key)
            .unwrap_or(Vec::new(&env));
 
        if let Some(last) = history.last() {
            if mileage < last.mileage {
                return Err(VehicleError::InvalidMileage);
            }
        }
 
        let event = HistoryEvent {
            event_type,
            description,
            mileage,
            timestamp:   env.ledger().timestamp(),
            recorded_by: caller,
        };
 
        history.push_back(event);
        vehicle.total_events += 1;
 
        env.storage().persistent().set(&history_key, &history);
        env.storage().persistent().set(&vin, &vehicle);
 
        env.events().publish(
            (VEHICLE, symbol_short!("event_add")),
            vin,
        );
 
        Ok(())
    }
 
    // ── Ownership transfer ───────────────────
 
    /// Transfer vehicle ownership to a new address.
    /// Must be called (and signed) by the current owner.
    pub fn transfer_ownership(
        env:       Env,
        vin:       String,
        new_owner: Address,
    ) -> Result<(), VehicleError> {
        let mut vehicle: Vehicle = env
            .storage()
            .persistent()
            .get(&vin)
            .ok_or(VehicleError::VehicleNotFound)?;
 
        // Only current owner can initiate transfer
        vehicle.owner.require_auth();
 
        // Stolen vehicles cannot be transferred
        if vehicle.is_stolen {
            return Err(VehicleError::VehicleIsStolen);
        }
 
        let previous_owner = vehicle.owner.clone();
        vehicle.owner = new_owner.clone();
        env.storage().persistent().set(&vin, &vehicle);
 
        env.events().publish(
            (VEHICLE, symbol_short!("transfer")),
            (vin, previous_owner, new_owner),
        );
 
        Ok(())
    }
 
    // ── Stolen flag ──────────────────────────
 
    /// Mark a vehicle as stolen. Admin-only.
    pub fn mark_stolen(env: Env, vin: String) -> Result<(), VehicleError> {
        let admin: Address = env.storage().instance().get(&ADMIN).unwrap();
        admin.require_auth();
 
        let mut vehicle: Vehicle = env
            .storage()
            .persistent()
            .get(&vin)
            .ok_or(VehicleError::VehicleNotFound)?;
 
        vehicle.is_stolen = true;
        env.storage().persistent().set(&vin, &vehicle);
 
        env.events().publish(
            (VEHICLE, symbol_short!("stolen")),
            vin,
        );
 
        Ok(())
    }
 
    /// Clear the stolen flag (e.g. vehicle recovered). Admin-only.
    pub fn clear_stolen(env: Env, vin: String) -> Result<(), VehicleError> {
        let admin: Address = env.storage().instance().get(&ADMIN).unwrap();
        admin.require_auth();
 
        let mut vehicle: Vehicle = env
            .storage()
            .persistent()
            .get(&vin)
            .ok_or(VehicleError::VehicleNotFound)?;
 
        vehicle.is_stolen = false;
        env.storage().persistent().set(&vin, &vehicle);
 
        Ok(())
    }
 
    // ── Queries ──────────────────────────────
 
    /// Fetch core vehicle metadata.
    pub fn get_vehicle(env: Env, vin: String) -> Option<Vehicle> {
        env.storage().persistent().get(&vin)
    }
 
    /// Fetch the full history log for a VIN.
    pub fn get_history(env: Env, vin: String) -> Vec<HistoryEvent> {
        let history_key = (HISTORY, vin);
        env.storage()
            .persistent()
            .get(&history_key)
            .unwrap_or(Vec::new(&env))
    }
 
    /// Fetch a single event by index (0-based).
    pub fn get_event(env: Env, vin: String, index: u32) -> Option<HistoryEvent> {
        let history_key = (HISTORY, vin);
        let history: Vec<HistoryEvent> = env
            .storage()
            .persistent()
            .get(&history_key)
            .unwrap_or(Vec::new(&env));
        history.get(index)
    }
 
    /// Quick stolen-status check.
    pub fn is_stolen(env: Env, vin: String) -> bool {
        let vehicle: Option<Vehicle> = env.storage().persistent().get(&vin);
        vehicle.map_or(false, |v| v.is_stolen)
    }
 
    /// Total number of history events for a vehicle.
    pub fn total_events(env: Env, vin: String) -> u32 {
        let vehicle: Option<Vehicle> = env.storage().persistent().get(&vin);
        vehicle.map_or(0, |v| v.total_events)
    }
}
 
// ─────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};
 
    fn setup() -> (Env, VehicleHistoryTrackerClient<'static>, Address, Address) {
        let env    = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, VehicleHistoryTracker);
        let client      = VehicleHistoryTrackerClient::new(&env, &contract_id);
        let admin       = Address::generate(&env);
        let owner       = Address::generate(&env);
        client.initialize(&admin);
        (env, client, admin, owner)
    }
 
    #[test]
    fn test_register_and_get() {
        let (env, client, _admin, owner) = setup();
        let vin   = String::from_str(&env, "1HGCM82633A123456");
        let make  = String::from_str(&env, "Honda");
        let model = String::from_str(&env, "Accord");
 
        client.register_vehicle(&owner, &vin, &make, &model, &2020);
 
        let v = client.get_vehicle(&vin).unwrap();
        assert_eq!(v.make,  make);
        assert_eq!(v.model, model);
        assert_eq!(v.year,  2020);
        assert_eq!(v.owner, owner);
    }
 
    #[test]
    fn test_add_history_event() {
        let (env, client, _admin, owner) = setup();
        let vin = String::from_str(&env, "1HGCM82633A654321");
 
        client.register_vehicle(
            &owner,
            &vin,
            &String::from_str(&env, "Toyota"),
            &String::from_str(&env, "Camry"),
            &2019,
        );
 
        client.add_history_event(
            &owner,
            &vin,
            &String::from_str(&env, "SERVICE"),
            &String::from_str(&env, "Oil change and tyre rotation"),
            &15_000,
        );
 
        assert_eq!(client.total_events(&vin), 1);
        let event = client.get_event(&vin, &0).unwrap();
        assert_eq!(event.mileage, 15_000);
    }
 
    #[test]
    fn test_ownership_transfer() {
        let (env, client, _admin, owner) = setup();
        let new_owner = Address::generate(&env);
        let vin = String::from_str(&env, "5YJ3E1EA8JF000001");
 
        client.register_vehicle(
            &owner,
            &vin,
            &String::from_str(&env, "Tesla"),
            &String::from_str(&env, "Model 3"),
            &2022,
        );
 
        client.transfer_ownership(&vin, &new_owner);
        let v = client.get_vehicle(&vin).unwrap();
        assert_eq!(v.owner, new_owner);
    }
 
    #[test]
    fn test_stolen_flag() {
        let (env, client, admin, owner) = setup();
        let vin = String::from_str(&env, "WBAFR7C51BC123456");
 
        client.register_vehicle(
            &owner,
            &vin,
            &String::from_str(&env, "BMW"),
            &String::from_str(&env, "535i"),
            &2021,
        );
 
        assert!(!client.is_stolen(&vin));
        client.mark_stolen(&vin);
        assert!(client.is_stolen(&vin));
        client.clear_stolen(&vin);
        assert!(!client.is_stolen(&vin));
 
        let _ = admin; // suppress unused warning
    }
 
    #[test]
    #[should_panic]
    fn test_double_registration_fails() {
        let (env, client, _admin, owner) = setup();
        let vin   = String::from_str(&env, "DUPLICATE000000001");
        let make  = String::from_str(&env, "Ford");
        let model = String::from_str(&env, "F-150");
 
        client.register_vehicle(&owner, &vin, &make, &model, &2023);
        client.register_vehicle(&owner, &vin, &make, &model, &2023); // should panic
    }
 
    #[test]
    #[should_panic]
    fn test_decreasing_mileage_rejected() {
        let (env, client, _admin, owner) = setup();
        let vin = String::from_str(&env, "MILEAGE0000000001");
 
        client.register_vehicle(
            &owner,
            &vin,
            &String::from_str(&env, "Chevrolet"),
            &String::from_str(&env, "Silverado"),
            &2020,
        );
 
        client.add_history_event(
            &owner, &vin,
            &String::from_str(&env, "SERVICE"),
            &String::from_str(&env, "First service"),
            &50_000,
        );
        client.add_history_event(
            &owner, &vin,
            &String::from_str(&env, "SERVICE"),
            &String::from_str(&env, "Odometer rollback attempt"),
            &10_000, // lower than previous — must fail
        );
    }
}
 