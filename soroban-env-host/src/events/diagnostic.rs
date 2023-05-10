use std::str::FromStr;

use soroban_env_common::{
    xdr::{ContractEventType, Hash, ScBytes, ScString, StringM},
    BytesObject, EnvBase, Symbol, SymbolSmall, VecObject, Error,
};

use crate::host_object::HostVec;
use crate::{budget::AsBudget, host::Frame, Host, HostError, RawVal};

use super::{InternalContractEvent, InternalEvent, InternalEventsBuffer};

#[derive(Clone, Default)]
pub enum DiagnosticLevel {
    #[default]
    None,
    Debug,
}

/// None of these functions are metered, which is why they're behind the is_debug check
impl Host {
    pub fn set_diagnostic_level(&self, diagnostic_level: DiagnosticLevel) {
        *self.0.diagnostic_level.borrow_mut() = diagnostic_level;
    }

    pub fn is_debug(&self) -> bool {
        matches!(*self.0.diagnostic_level.borrow(), DiagnosticLevel::Debug)
    }

    fn hash_to_bytesobj(&self, hash: &Hash) -> Result<BytesObject, HostError> {
        self.add_host_object::<ScBytes>(hash.as_slice().to_vec().try_into()?)
    }

    /// Records a `System` contract event. `topics` is expected to be a `SCVec`
    /// length <= 4 that cannot contain `Vec`, `Map`, or `Bytes` with length > 32
    pub fn system_event(&self, topics: VecObject, data: RawVal) -> Result<(), HostError> {
        self.record_contract_event(ContractEventType::System, topics, data)?;
        Ok(())
    }

    pub(crate) fn record_system_debug_contract_event(
        &self,
        type_: ContractEventType,
        contract_id: Option<BytesObject>,
        topics: VecObject,
        data: RawVal,
    ) -> Result<(), HostError> {
        let ce = InternalContractEvent {
            type_,
            contract_id,
            topics,
            data,
        };
        self.with_events_mut(|events| {
            Ok(events.record(InternalEvent::StructuredDebug(ce), self.as_budget()))
        })?
    }

    // Will not return error if frame is missing
    pub(crate) fn get_current_contract_id_unmetered(&self) -> Result<Option<Hash>, HostError> {
        self.with_current_frame_opt(|frame| match frame {
            Some(Frame::ContractVM(vm, _, _)) => Ok(Some(vm.contract_id.clone())),
            Some(Frame::HostFunction(_)) => Ok(None),
            Some(Frame::Token(id, _, _)) => Ok(Some(id.clone())),
            #[cfg(any(test, feature = "testutils"))]
            Some(Frame::TestContract(tc)) => Ok(Some(tc.id.clone())),
            None => Ok(None),
        })
    }

    fn current_contract_bytesobject_option(&self) -> Result<Option<BytesObject>, HostError>
    {
        if let Some(calling_hash) = self.get_current_contract_id_unmetered()? {
            Ok(Some(self.hash_to_bytesobj(&calling_hash)?))
        } else {
            Ok(None)
        }
    }

    pub fn err_diagnostics(
        &self,
        events: &mut InternalEventsBuffer,
        error: Error,
        msg: &str,
        args: &[RawVal]
    ) -> Result<(), HostError>
    {
        const ERROR_SYM: SymbolSmall = SymbolSmall::try_from_str("error").unwrap();

        if !self.is_debug() {
            return Ok(());
        }

        self.as_budget().with_free_budget(|| {
            let type_ = ContractEventType::Diagnostic;
            let contract_id = self.current_contract_bytesobject_option()?;
            let topics: Vec<RawVal> = vec![ERROR_SYM.to_raw(), error.to_raw()];
            let topics = self.add_host_object(HostVec::from_vec(topics)?)?;
            let msg = self.add_host_object(ScString(StringM::from_str(msg)?))?;
            let data = std::iter::once(msg.to_raw())
                .chain(args.iter().cloned());
            let data = self.add_host_object(HostVec::from_exact_iter(data, self.as_budget())?)?.to_raw();

            let ce = InternalContractEvent { type_, contract_id, topics, data };
            events.record(InternalEvent::StructuredDebug(ce), self.as_budget())
        })
    }

    // Emits an event with topic = ["fn_call", called_contract_id, function_name] and
    // data = [arg1, args2, ...]
    // Should called prior to opening a frame for the next call so the calling contract can be inferred correctly
    pub fn fn_call_diagnostics(
        &self,
        called_contract_id: &Hash,
        func: &Symbol,
        args: &[RawVal],
    ) -> Result<(), HostError> {
        if !self.is_debug() {
            return Ok(());
        }

        let calling_contract = self.current_contract_bytesobject_option()?;

        self.as_budget().with_free_budget(|| {
            let topics: Vec<RawVal> = vec![
                SymbolSmall::try_from_str("fn_call")?.into(),
                self.hash_to_bytesobj(called_contract_id)?.into(),
                func.into(),
            ];

            self.record_system_debug_contract_event(
                ContractEventType::Diagnostic,
                calling_contract,
                self.add_host_object(HostVec::from_vec(topics)?)?,
                self.vec_new_from_slice(args)?.into(),
            )
        })
    }

    // Emits an event with topic = ["fn_return", contract_id, function_name] and
    // data = [return_val]
    pub fn fn_return_diagnostics(
        &self,
        contract_id: &Hash,
        func: &Symbol,
        res: &RawVal,
    ) -> Result<(), HostError> {
        if !self.is_debug() {
            return Ok(());
        }

        self.as_budget().with_free_budget(|| {
            let topics: Vec<RawVal> =
                vec![SymbolSmall::try_from_str("fn_return")?.into(), func.into()];

            self.record_system_debug_contract_event(
                ContractEventType::Diagnostic,
                Some(self.hash_to_bytesobj(contract_id)?),
                self.add_host_object(HostVec::from_vec(topics)?)?,
                *res,
            )
        })
    }
}
