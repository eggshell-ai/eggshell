<?php

namespace App\Validator;

use Symfony\Component\Validator\Constraint;

#[\Attribute]
class Phone extends Constraint
{
    public string $message = 'The value "{{ string }}" is not a valid phone number. It must be in E.164 format (e.g. +14155552671).';

    // all configurable options must be passed to the constructor
    public function __construct(?string $message = null, ?array $groups = null, $payload = null)
    {
        $this->message = $message ?? $this->message;

        parent::__construct(null, $groups, $payload);
    }
}
