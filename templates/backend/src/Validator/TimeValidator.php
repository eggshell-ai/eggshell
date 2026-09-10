<?php

namespace App\Validator;

use Symfony\Component\Validator\Constraint;
use Symfony\Component\Validator\ConstraintValidator;
use Symfony\Component\Validator\Exception\UnexpectedTypeException;
use Symfony\Component\Validator\Exception\UnexpectedValueException;

class TimeValidator extends ConstraintValidator
{
    /**
     * 24-hour time format HH:MM:SS with valid ranges
     * (00-23 hours, 00-59 minutes, 00-59 seconds)
     */
    private const TIME_PATTERN = '/^(?:[01]\d|2[0-3]):[0-5]\d:[0-5]\d$/';

    public function validate(mixed $value, Constraint $constraint): void
    {
        if (!$constraint instanceof Time) {
            throw new UnexpectedTypeException($constraint, Time::class);
        }

        if (null === $value || '' === $value) {
            return;
        }

        if (!is_string($value)) {
            throw new UnexpectedValueException($value, 'string');
        }

        if (!preg_match(self::TIME_PATTERN, $value)) {
            $this->context->buildViolation($constraint->message)
                ->setParameter('{{ string }}', $value)
                ->addViolation();
        }
    }
}
