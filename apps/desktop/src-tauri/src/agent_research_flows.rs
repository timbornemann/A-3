//! Ask and Plan use the same bounded Fast Index reads and current-source gate as Agent.
use super::*;

impl AgentAskResearcher {
    pub(super) async fn read_function_flow(
        &self,
        project: &ProjectIdentity,
        published: &a3_domain::PublishedIndex,
        turn: &AskResearchTurn,
        state: &mut AskResearchWorkingSet,
        action: &AskResearchAction,
        control: &JobContext,
    ) -> Result<String, AgentSessionManagerFailure> {
        let AskResearchAction::InspectFunctionFlow {
            source_ordinal,
            call_path,
            view,
        } = action
        else {
            return Err(AgentSessionManagerFailure::InvalidOutput);
        };
        let Some(flows) = &self.flows else {
            state.observe_access(a3_domain::ResearchAccessOutcome::Unavailable);
            return Ok("Ablaufanalyse ist für diesen Index nicht verfügbar.".to_owned());
        };
        let source = state
            .sources
            .get(usize::from(source_ordinal.saturating_sub(1)))
            .ok_or(AgentSessionManagerFailure::InvalidOutput)?;
        let candidates = published
            .publication()
            .graph()
            .symbols()
            .iter()
            .filter(|symbol| {
                symbol.revision() == source.revision()
                    && source.symbol() == Some(symbol.parsed().name().as_str())
                    && source.range().is_none_or(|r| {
                        r.contains(symbol.parsed().selection_range())
                            || symbol.parsed().declaration_range().contains(r)
                    })
            })
            .take(2)
            .collect::<Vec<_>>();
        if candidates.len() != 1 {
            state.observe_access(a3_domain::ResearchAccessOutcome::Unresolved);
            return Ok("Bitte zuerst eine eindeutige Funktionsquelle suchen; diese Quelle bezeichnet keinen eindeutigen Ablauf.".to_owned());
        }
        let request =
            a3_domain::FunctionFlowReadRequest::new(candidates[0].id(), call_path.clone(), *view)
                .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?;
        state.event_sequence = state.event_sequence.saturating_add(1);
        self.append_running_event(
            project,
            turn,
            state.event_sequence,
            AskResearchPhase::Reading,
            "Statische Schritte und Wertverknüpfungen einer bekannten Quelle verfolgen",
            Some(&format!("Quelle S{source_ordinal}")),
            AskResearchCompleteness::NotApplicable,
        )
        .await?;
        let Some(document) = flows
            .read_document(
                project,
                published.run().id(),
                &request,
                &ConversationIndexControl { context: control },
            )
            .await
            .map_err(|_| AgentSessionManagerFailure::Unavailable)?
        else {
            state.observe_access(a3_domain::ResearchAccessOutcome::Unavailable);
            return Ok(
                "Ablaufdaten sind nicht mehr aktuell oder das Ziel ist nicht eindeutig auflösbar."
                    .to_owned(),
            );
        };
        let mut mapping = String::new();
        let mut verified = std::collections::BTreeSet::new();
        for (ordinal, evidence) in document.evidence.iter().enumerate() {
            // Safe source reads compare live content hashes before any static fact reaches a model.
            // The working set may already contain a page; its deduplication is not a freshness check.
            if verified.insert(evidence.revision().path().clone()) {
                let probe = AgentFileInspection::new(
                    evidence.revision().path().clone(),
                    AgentFileStartLine::new(1)
                        .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?,
                    AgentFileLineCount::new(1)
                        .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?,
                );
                WorkspaceAgentSourceReader
                    .read_page(project, evidence.revision(), &probe, control)
                    .await
                    .map_err(|_| AgentSessionManagerFailure::Unavailable)?;
            }
            self.add_and_read_source(
                project,
                turn,
                state,
                evidence.revision().clone(),
                Some(evidence.range()),
                None,
                AskResearchSourceKind::Relationship,
                AskResearchSelectionReason::Relationship,
                control,
            )
            .await?;
            let Some(source) = state.sources.iter().find(|s| {
                s.revision() == evidence.revision()
                    && s.range().is_some_and(|r| {
                        r.contains(evidence.range()) || evidence.range().contains(r)
                    })
            }) else {
                state.observe_access(a3_domain::ResearchAccessOutcome::Limited);
                return Ok("Ablaufergebnis zurückgehalten: Nicht alle aktuellen Quellbelege konnten im Recherchebudget gesichert werden.".to_owned());
            };
            mapping.push_str(&format!(
                "flow_source={ordinal} entspricht S{}\n",
                source.ordinal()
            ));
        }
        let Some(current) = self
            .index
            .load_current_index(project, control)
            .await
            .map_err(|_| AgentSessionManagerFailure::Unavailable)?
        else {
            return Err(AgentSessionManagerFailure::Unavailable);
        };
        if current.run().id() != published.run().id() {
            return Err(AgentSessionManagerFailure::Unavailable);
        }
        Ok(format!("{mapping}{}", document.text))
    }
}
