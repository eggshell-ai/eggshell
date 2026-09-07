<?php

namespace App\Validator;

use Symfony\Component\Validator\Constraint;
use Symfony\Component\Validator\ConstraintValidator;
use Symfony\Component\Validator\Exception\UnexpectedTypeException;
use Symfony\Component\Validator\Exception\UnexpectedValueException;

class OneOfValidator extends ConstraintValidator
{
    public function validate(mixed $value, Constraint $constraint): void
    {
        if (!$constraint instanceof OneOf) {
            throw new UnexpectedTypeException($constraint, OneOf::class);
        }

        if (null === $value || '' === $value) {
            return;
        }

        if (!is_scalar($value) && !(\is_object($value) && \method_exists($value, '__toString'))) {
            throw new UnexpectedValueException($value, 'scalar');
        }

        if (!\in_array($value, $constraint->choices, true)) {
            $this->context->buildViolation($constraint->message)
                ->setParameter('{{ string }}', (string) $value)
                ->setParameter('{{ choices }}', \implode(', ', $constraint->choices))
                ->addViolation();
        }
    }
}
