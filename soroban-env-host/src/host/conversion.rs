use super::metered_clone::MeteredClone;
use crate::xdr::{
    Hash, LedgerKey, LedgerKeyContractData, ScHostFnErrorCode, ScHostObjErrorCode,
    ScHostValErrorCode, ScStatic, ScVal, ScVec, Uint256,
};
use crate::{
    budget::CostType, events::DebugError, host_object::HostVec, Host, HostError, Object, RawVal,
};
use ed25519_dalek::{PublicKey, Signature, SIGNATURE_LENGTH};
use sha2::{Digest, Sha256};
use soroban_env_common::xdr::{self, AccountId, ScObject};
use soroban_env_common::TryFromVal;

impl Host {
    // Notes on metering: free
    pub(crate) fn usize_to_u32(&self, u: usize, msg: &'static str) -> Result<u32, HostError> {
        match u32::try_from(u) {
            Ok(v) => Ok(v),
            Err(_) => Err(self.err_general(msg)), // FIXME: need error status
        }
    }

    // Notes on metering: free
    pub(crate) fn usize_to_rawval_u32(&self, u: usize) -> Result<RawVal, HostError> {
        match u32::try_from(u) {
            Ok(v) => Ok(v.into()),
            Err(_) => Err(self.err_status(ScHostValErrorCode::U32OutOfRange)),
        }
    }

    // Notes on metering: free
    pub(crate) fn usize_from_rawval_u32_input(
        &self,
        name: &'static str,
        r: RawVal,
    ) -> Result<usize, HostError> {
        self.u32_from_rawval_input(name, r).map(|u| u as usize)
    }

    // Notes on metering: free
    pub(crate) fn u32_from_rawval_input(
        &self,
        name: &'static str,
        r: RawVal,
    ) -> Result<u32, HostError> {
        match u32::try_from(r) {
            Ok(v) => Ok(v),
            Err(cvt) => Err(self.err(
                DebugError::new(ScHostFnErrorCode::InputArgsWrongType)
                    .msg("unexpected RawVal {} for input '{}', need U32")
                    .arg(r)
                    .arg(name),
            )),
        }
    }

    pub(crate) fn to_u256_from_account(
        &self,
        account_id: &AccountId,
    ) -> Result<Uint256, HostError> {
        let crate::xdr::PublicKey::PublicKeyTypeEd25519(ed25519) =
            account_id.metered_clone(&self.0.budget)?.0;
        Ok(ed25519)
    }

    pub(crate) fn to_u256(&self, a: Object) -> Result<Uint256, HostError> {
        self.visit_obj(a, |bytes: &Vec<u8>| {
            self.charge_budget(CostType::BytesClone, 32)?;
            bytes
                .try_into()
                .map_err(|_| self.err_general("bad u256 length"))
        })
    }

    // Notes on metering: free
    pub(crate) fn u8_from_rawval_input(
        &self,
        name: &'static str,
        r: RawVal,
    ) -> Result<u8, HostError> {
        let u = self.u32_from_rawval_input(name, r)?;
        match u8::try_from(u) {
            Ok(v) => Ok(v),
            Err(cvt) => Err(self.err(
                DebugError::new(ScHostFnErrorCode::InputArgsWrongType)
                    .msg("unexpected RawVal {} for input '{}', need u32 no greater than 255")
                    .arg(r)
                    .arg(name),
            )),
        }
    }

    pub(crate) fn hash_from_obj_input(
        &self,
        name: &'static str,
        hash: Object,
    ) -> Result<Hash, HostError> {
        self.fixed_length_bytes_from_obj_input::<Hash, 32>(name, hash)
    }

    pub(crate) fn uint256_from_obj_input(
        &self,
        name: &'static str,
        u256: Object,
    ) -> Result<Uint256, HostError> {
        self.fixed_length_bytes_from_obj_input::<Uint256, 32>(name, u256)
    }

    pub(crate) fn signature_from_bytes(
        &self,
        name: &'static str,
        bytes: &[u8],
    ) -> Result<Signature, HostError> {
        self.fixed_length_bytes_from_slice::<Signature, SIGNATURE_LENGTH>(name, bytes)
    }

    pub(crate) fn signature_from_obj_input(
        &self,
        name: &'static str,
        sig: Object,
    ) -> Result<Signature, HostError> {
        self.fixed_length_bytes_from_obj_input::<Signature, SIGNATURE_LENGTH>(name, sig)
    }

    fn fixed_length_bytes_from_slice<T, const N: usize>(
        &self,
        name: &'static str,
        bytes_arr: &[u8],
    ) -> Result<T, HostError>
    where
        T: From<[u8; N]>,
    {
        match <[u8; N]>::try_from(bytes_arr) {
            Ok(arr) => {
                self.charge_budget(CostType::BytesClone, N as u64)?;
                Ok(arr.into())
            }
            Err(cvt) => Err(self.err(
                // TODO: This is a wrong error code to use here, we should replace
                // it with a more generic one.
                DebugError::new(ScHostObjErrorCode::ContractHashWrongLength) // TODO: this should be renamed to be more generic
                    .msg("{} has wrong length for input '{}'")
                    .arg(std::any::type_name::<T>())
                    .arg(name),
            )),
        }
    }

    fn fixed_length_bytes_from_obj_input<T, const N: usize>(
        &self,
        name: &'static str,
        obj: Object,
    ) -> Result<T, HostError>
    where
        T: From<[u8; N]>,
    {
        self.visit_obj(obj, |bytes: &Vec<u8>| {
            self.fixed_length_bytes_from_slice(name, bytes)
        })
    }

    pub(crate) fn ed25519_pub_key_from_bytes(&self, bytes: &[u8]) -> Result<PublicKey, HostError> {
        self.charge_budget(CostType::ComputeEd25519PubKey, bytes.len() as u64)?;
        PublicKey::from_bytes(bytes).map_err(|_| {
            self.err_status_msg(ScHostObjErrorCode::UnexpectedType, "invalid public key")
        })
    }

    pub fn ed25519_pub_key_from_obj_input(&self, k: Object) -> Result<PublicKey, HostError> {
        self.visit_obj(k, |bytes: &Vec<u8>| self.ed25519_pub_key_from_bytes(bytes))
    }

    pub(crate) fn account_id_from_bytes(&self, k: Object) -> Result<AccountId, HostError> {
        self.visit_obj(k, |bytes: &Vec<u8>| {
            Ok(AccountId(xdr::PublicKey::PublicKeyTypeEd25519(
                self.fixed_length_bytes_from_slice("account_id", bytes.as_slice())?,
            )))
        })
    }

    pub fn sha256_hash_from_bytes_input(&self, x: Object) -> Result<Vec<u8>, HostError> {
        self.visit_obj(x, |bytes: &Vec<u8>| {
            self.charge_budget(CostType::ComputeSha256Hash, bytes.len() as u64)?;
            let hash = Sha256::digest(bytes).as_slice().to_vec();
            if hash.len() != 32 {
                return Err(self.err_general("incorrect hash size"));
            }
            Ok(hash)
        })
    }

    /// Converts a [`RawVal`] to an [`ScVal`] and combines it with the currently-executing
    /// [`ContractID`] to produce a [`Key`], that can be used to access ledger [`Storage`].
    // Notes on metering: covered by components.
    pub fn storage_key_from_rawval(&self, k: RawVal) -> Result<LedgerKey, HostError> {
        Ok(LedgerKey::ContractData(LedgerKeyContractData {
            contract_id: self.get_current_contract_id_internal()?,
            key: self.from_host_val(k)?,
        }))
    }

    pub(crate) fn storage_key_for_contract(&self, contract_id: Hash, key: ScVal) -> LedgerKey {
        LedgerKey::ContractData(LedgerKeyContractData { contract_id, key })
    }

    pub fn storage_key_from_scval(&self, key: ScVal) -> Result<LedgerKey, HostError> {
        Ok(LedgerKey::ContractData(LedgerKeyContractData {
            contract_id: self.get_current_contract_id_internal()?,
            key,
        }))
    }

    // Notes on metering: covered by components.
    pub fn contract_data_key_from_rawval(&self, k: RawVal) -> Result<LedgerKey, HostError> {
        let key_scval = self.from_host_val(k)?;
        match &key_scval {
            ScVal::Static(ScStatic::LedgerKeyContractCode) => {
                return Err(self.err_status_msg(
                    ScHostFnErrorCode::InputArgsInvalid,
                    "cannot update contract code",
                ));
            }
            ScVal::Object(Some(ScObject::NonceKey(_))) => {
                return Err(self.err_status_msg(
                    ScHostFnErrorCode::InputArgsInvalid,
                    "cannot access internal nonce",
                ));
            }
            _ => (),
        };

        self.storage_key_from_scval(key_scval)
    }

    /// Converts a binary search result into a u64. `res` is `Some(index)`
    /// if the value was found at `index`, or `Err(index)` if the value was not found
    /// and would've needed to be inserted at `index`.
    /// Returns a Some(res_u64) where :
    /// - the high-32 bits is 0x0001 if element existed or 0x0000 if it didn't
    /// - the low-32 bits contains the u32 representation of the `index`
    /// Err(_) if the `index` fails to be converted to an u32.
    pub(crate) fn u64_from_binary_search_result(
        &self,
        res: Result<usize, usize>,
    ) -> Result<u64, HostError> {
        match res {
            Ok(u) => {
                let v = self.usize_to_u32(u, "outside range")?;
                Ok(u64::from(v) | (1 << u32::BITS))
            }
            Err(u) => {
                let v = self.usize_to_u32(u, "outside range")?;
                Ok(u64::from(v))
            }
        }
    }

    pub(crate) fn call_args_from_obj(&self, args: Object) -> Result<Vec<RawVal>, HostError> {
        self.visit_obj(args, |hv: &HostVec| {
            // Metering: free
            Ok(hv.iter().cloned().collect())
        })
    }

    // Metering: free?
    pub(crate) fn call_args_to_scvec(&self, args: Object) -> Result<ScVec, HostError> {
        self.visit_obj(args, |hv: &HostVec| self.rawvals_to_scvec(hv.iter()))
    }

    // Metering: free?
    pub(crate) fn rawvals_to_scvec(
        &self,
        raw_vals: std::slice::Iter<RawVal>,
    ) -> Result<ScVec, HostError> {
        Ok(ScVec(
            raw_vals
                .map(|v| {
                    ScVal::try_from_val(self, v)
                        .map_err(|_| self.err_general("couldn't convert RawVal"))
                })
                .collect::<Result<Vec<ScVal>, HostError>>()?
                .try_into()
                .map_err(|_| self.err_general("too many args"))?,
        ))
    }

    pub(crate) fn scvals_to_rawvals(&self, sc_vals: &[ScVal]) -> Result<Vec<RawVal>, HostError> {
        sc_vals
            .iter()
            .map(|scv| self.to_host_val(scv))
            .collect::<Result<Vec<RawVal>, HostError>>()
    }

    pub(crate) fn obj_from_internal_contract_id(&self) -> Result<Option<Object>, HostError> {
        Ok(self
            .get_current_contract_id_internal()
            .map(|id| self.add_host_object::<Vec<u8>>(id.as_slice().to_vec()))?
            .ok())
    }
}
