//! A solver that uses [highs](https://docs.rs/highs), a parallel C++ solver.

use highs::HighsModelStatus;

use crate::solvers::{ObjectiveDirection, ResolutionError, Solution, SolverModel};
use crate::{
    constraint::ConstraintReference,
    dual::Dual,
    solvers::SolutionWithDual,
    variable::{UnsolvedProblem, VariableDefinition},
};
use crate::{Constraint, IntoAffineExpression, Variable};

/// The [highs](https://docs.rs/highs) solver,
/// to be used with [UnsolvedProblem::using].
pub fn highs(to_solve: UnsolvedProblem) -> HighsProblem {
    let mut highs_problem = highs::RowProblem::default();
    let sense = match to_solve.direction {
        ObjectiveDirection::Maximisation => highs::Sense::Maximise,
        ObjectiveDirection::Minimisation => highs::Sense::Minimise,
    };
    let mut columns = Vec::with_capacity(to_solve.variables.len());
    for (var, &VariableDefinition { min, max, .. }) in to_solve.variables.iter_variables_with_def()
    {
        let &col_factor = to_solve
            .objective
            .linear
            .coefficients
            .get(&var)
            .unwrap_or(&0.);
        let col = highs_problem.add_column(col_factor, min..max);
        columns.push(col);
    }
    HighsProblem {
        sense,
        highs_problem,
        columns,
        n_constraints: 0,
    }
}

/// A HiGHS model
#[derive(Debug)]
pub struct HighsProblem {
    sense: highs::Sense,
    highs_problem: highs::RowProblem,
    columns: Vec<highs::Col>,
    n_constraints: usize,
}

impl HighsProblem {
    /// Get a highs model for this problem
    pub fn into_inner(self) -> highs::Model {
        self.highs_problem.optimise(self.sense)
    }

    /// Default implementation for adding a constraint to the Problem
    fn put_constraint(&mut self, constraint: Constraint) {
        let upper_bound = -constraint.expression.constant();
        let columns = &self.columns;
        let factors = constraint
            .expression
            .linear_coefficients()
            .into_iter()
            .map(|(variable, factor)| (columns[variable.index()], factor));
        if constraint.is_equality {
            self.highs_problem
                .add_row(upper_bound..=upper_bound, factors);
        } else {
            self.highs_problem.add_row(..=upper_bound, factors);
        }
        self.n_constraints += 1;
    }
}

impl SolverModel for HighsProblem {
    type Solution = HighsSolution;
    type Error = ResolutionError;

    fn solve(self) -> Result<Self::Solution, Self::Error> {
        let model = self.into_inner();
        let solved = model.solve();
        match solved.status() {
            HighsModelStatus::NotSet => Err(ResolutionError::Other("NotSet")),
            HighsModelStatus::LoadError => Err(ResolutionError::Other("LoadError")),
            HighsModelStatus::ModelError => Err(ResolutionError::Other("ModelError")),
            HighsModelStatus::PresolveError => Err(ResolutionError::Other("PresolveError")),
            HighsModelStatus::SolveError => Err(ResolutionError::Other("SolveError")),
            HighsModelStatus::PostsolveError => Err(ResolutionError::Other("PostsolveError")),
            HighsModelStatus::ModelEmpty => Err(ResolutionError::Other("ModelEmpty")),
            HighsModelStatus::PrimalInfeasible => Err(ResolutionError::Infeasible),
            HighsModelStatus::PrimalUnbounded => Err(ResolutionError::Unbounded),
            _ok_status => Ok(HighsSolution {
                solution: solved.get_solution(),
                dual_values: vec![],
                acquired: false,
            }),
        }
    }

    fn add_constraint(&mut self, c: Constraint) -> ConstraintReference {
        self.put_constraint(c);

        ConstraintReference {
            index: self.n_constraints - 1,
        }
    }
}

/// The solution to a highs problem
#[derive(Debug)]
pub struct HighsSolution {
    solution: highs::Solution,
    dual_values: Vec<f64>,
    acquired: bool,
}

impl HighsSolution {
    /// Returns the highs solution object. You can use it to fetch dual values
    pub fn into_inner(self) -> highs::Solution {
        self.solution
    }
}

impl Solution for HighsSolution {
    fn value(&self, variable: Variable) -> f64 {
        self.solution.columns()[variable.index()]
    }
}

impl SolutionWithDual for HighsSolution {
    fn dual(&self, constraint: ConstraintReference) -> f64 {
        self.dual_values[constraint.index]
    }
}

impl<'a> Dual<'_> for HighsSolution {
    fn get_dual(&mut self) -> &Self {
        if !self.acquired {
            self.dual_values = self.solution.dual_rows().to_vec();
            self.acquired = true;
        }

        self
    }
}
