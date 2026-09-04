<?php

namespace App\Validator;

use Doctrine\ORM\EntityManagerInterface;
use Symfony\Component\Validator\Constraint;
use Symfony\Component\Validator\ConstraintValidator;
use Symfony\Component\Validator\Exception\UnexpectedTypeException;

class UniqueValidator extends ConstraintValidator
{
    public function __construct(private readonly EntityManagerInterface $entityManager)
    {
    }

    public function validate(mixed $value, Constraint $constraint): void
    {
        if (!$constraint instanceof Unique) {
            throw new UnexpectedTypeException($constraint, Unique::class);
        }

        if (null === $value || '' === $value) {
            return;
        }

        $object = $this->context->getObject();
        $property = $this->context->getPropertyName();

        if ($object === null || $property === null) {
            return;
        }

        $class = get_class($object);
        $metadata = $this->entityManager->getClassMetadata($class);

        // Only check real, non-association entity fields
        if (!$metadata->hasField($property) || $metadata->hasAssociation($property)) {
            return;
        }

        $queryBuilder = $this->entityManager->getRepository($class)
            ->createQueryBuilder('e')
            ->select('COUNT(e.id)')
            ->where('e.' . $property . ' = :value')
            ->setParameter('value', $value);

        // Exclude the current record when updating
        if (property_exists($object, 'id') && $object->id !== null) {
            $queryBuilder
                ->andWhere('e.id != :id')
                ->setParameter('id', $object->id);
        }

        $count = (int) $queryBuilder->getQuery()->getSingleScalarResult();

        if ($count > 0) {
            $this->context->buildViolation($constraint->message)
                ->setParameter('{{ string }}', (string) $value)
                ->addViolation();
        }
    }
}
